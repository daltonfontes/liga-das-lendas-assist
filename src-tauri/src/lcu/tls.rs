use std::sync::Arc;

use ring::signature::{self, UnparsedPublicKey, VerificationAlgorithm};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{ring as ring_provider, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{CertificateError, ClientConfig, DigitallySignedStruct, Error, SignatureScheme};
use x509_parser::prelude::{FromDer, X509Certificate};
use x509_parser::time::ASN1Time;
use x509_parser::x509::SubjectPublicKeyInfo;

/// Certificado raiz público da Riot que assina o LCU e a Live Client Data API.
pub const RIOT_CA_PEM: &[u8] = include_bytes!("../../certs/riotgames.pem");

/// Verificador fixado na CA da Riot.
///
/// Não usamos o verificador WebPKI padrão porque (1) o certificado é emitido para
/// "rclient" e conectamos em 127.0.0.1, e (2) a CA da Riot é X.509 v1, formato que o
/// webpki recusa. Em vez disso conferimos: assinatura do certificado pela CA,
/// validade e assinatura do handshake com a chave do certificado.
#[derive(Debug)]
struct RiotVerifier {
    /// SubjectPublicKeyInfo (DER) de cada CA confiável.
    ca_keys: Vec<Vec<u8>>,
}

fn bad(err: CertificateError) -> Error {
    Error::InvalidCertificate(err)
}

fn parse(cert: &[u8]) -> Result<X509Certificate<'_>, Error> {
    X509Certificate::from_der(cert)
        .map(|(_, c)| c)
        .map_err(|_| bad(CertificateError::BadEncoding))
}

const SCHEMES: &[SignatureScheme] = &[
    SignatureScheme::RSA_PSS_SHA256,
    SignatureScheme::RSA_PSS_SHA384,
    SignatureScheme::RSA_PSS_SHA512,
    SignatureScheme::RSA_PKCS1_SHA256,
    SignatureScheme::RSA_PKCS1_SHA384,
    SignatureScheme::RSA_PKCS1_SHA512,
    SignatureScheme::ECDSA_NISTP256_SHA256,
    SignatureScheme::ECDSA_NISTP384_SHA384,
    SignatureScheme::ED25519,
];

fn algorithm(scheme: SignatureScheme) -> Option<&'static dyn VerificationAlgorithm> {
    Some(match scheme {
        SignatureScheme::RSA_PSS_SHA256 => &signature::RSA_PSS_2048_8192_SHA256,
        SignatureScheme::RSA_PSS_SHA384 => &signature::RSA_PSS_2048_8192_SHA384,
        SignatureScheme::RSA_PSS_SHA512 => &signature::RSA_PSS_2048_8192_SHA512,
        SignatureScheme::RSA_PKCS1_SHA256 => &signature::RSA_PKCS1_2048_8192_SHA256,
        SignatureScheme::RSA_PKCS1_SHA384 => &signature::RSA_PKCS1_2048_8192_SHA384,
        SignatureScheme::RSA_PKCS1_SHA512 => &signature::RSA_PKCS1_2048_8192_SHA512,
        SignatureScheme::ECDSA_NISTP256_SHA256 => &signature::ECDSA_P256_SHA256_ASN1,
        SignatureScheme::ECDSA_NISTP384_SHA384 => &signature::ECDSA_P384_SHA384_ASN1,
        SignatureScheme::ED25519 => &signature::ED25519,
        _ => return None,
    })
}

impl RiotVerifier {
    fn verify_handshake(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        let alg = algorithm(dss.scheme).ok_or(Error::PeerMisbehaved(
            rustls::PeerMisbehaved::SignedHandshakeWithUnadvertisedSigScheme,
        ))?;
        let leaf = parse(cert)?;
        let key = &leaf.public_key().subject_public_key.data;
        UnparsedPublicKey::new(alg, key.as_ref())
            .verify(message, dss.signature())
            .map(|_| HandshakeSignatureValid::assertion())
            .map_err(|_| bad(CertificateError::BadSignature))
    }
}

impl ServerCertVerifier for RiotVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        let leaf = parse(end_entity)?;
        let signed_by_ca = self.ca_keys.iter().any(|der| {
            SubjectPublicKeyInfo::from_der(der)
                .is_ok_and(|(_, ca)| leaf.verify_signature(Some(&ca)).is_ok())
        });
        if !signed_by_ca {
            return Err(bad(CertificateError::UnknownIssuer));
        }
        let now = ASN1Time::from_timestamp(now.as_secs() as i64)
            .map_err(|_| bad(CertificateError::BadEncoding))?;
        if !leaf.validity().is_valid_at(now) {
            return Err(bad(CertificateError::Expired));
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.verify_handshake(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.verify_handshake(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        SCHEMES.to_vec()
    }
}

pub fn client_config_with_ca(ca_pem: &[u8]) -> anyhow::Result<Arc<ClientConfig>> {
    let mut ca_keys = Vec::new();
    for pem in x509_parser::pem::Pem::iter_from_buffer(ca_pem) {
        let pem = pem?;
        ca_keys.push(pem.parse_x509()?.tbs_certificate.subject_pki.raw.to_vec());
    }
    anyhow::ensure!(!ca_keys.is_empty(), "nenhum certificado de CA encontrado");
    let config = ClientConfig::builder_with_provider(Arc::new(ring_provider::default_provider()))
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(RiotVerifier { ca_keys }))
        .with_no_client_auth();
    Ok(Arc::new(config))
}

pub fn riot_client_config() -> anyhow::Result<Arc<ClientConfig>> {
    // Garante um provedor padrão para qualquer código que não receba a config explicitamente.
    let _ = CryptoProvider::install_default(ring_provider::default_provider());
    client_config_with_ca(RIOT_CA_PEM)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carrega_a_ca_da_riot() {
        assert!(riot_client_config().is_ok());
    }
}
