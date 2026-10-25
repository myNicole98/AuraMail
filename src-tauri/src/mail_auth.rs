#[derive(Clone, PartialEq, Eq)]
pub enum MailCredential {
    Password(String),
    AccessToken(String),
}

impl std::fmt::Debug for MailCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Password(_) => formatter.write_str("Password(***)"),
            Self::AccessToken(_) => formatter.write_str("AccessToken(***)"),
        }
    }
}

impl MailCredential {
    pub fn secret(&self) -> &str {
        match self {
            Self::Password(value) | Self::AccessToken(value) => value,
        }
    }

    pub fn is_oauth(&self) -> bool {
        matches!(self, Self::AccessToken(_))
    }
}
