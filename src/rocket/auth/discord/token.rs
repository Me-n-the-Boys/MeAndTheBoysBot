#![allow(dead_code)]
#[derive(Debug, Copy, Clone, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
#[serde(tag = "grant_type")]
pub enum TokenRequest<'a> {
    AuthorizationCode{
        code: &'a str,
        redirect_uri: &'a str,
    },
    RefreshToken{
        refresh_token: &'a str,
    },
}

#[derive(Debug, Copy, Clone, serde::Deserialize, serde::Serialize, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    Bearer,
}
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub struct Token {
    pub access_token: String,
    pub token_type: TokenType,
    #[serde(skip, default = "::chrono::Utc::now")]
    pub created: chrono::DateTime<chrono::Utc>,
    pub expires_in: u64,
    pub refresh_token: String,
    pub scope: String,
}

impl<'a> TokenRequest<'a> {
    pub const fn authorization_code(code: &'a str, redirect_uri: &'a str) -> Self {
        Self::AuthorizationCode {
            code,
            redirect_uri,
        }
    }

    pub const fn refresh_token(refresh_token: &'a str) -> Self {
        Self::RefreshToken {
            refresh_token,
        }
    }

    pub async fn request_token(&self, discord: &super::Discord) -> ::anyhow::Result<Token> {
        let response = discord.client.post("https://discord.com/api/oauth2/token")
            .form(self)
            .basic_auth(&discord.client_id, Some(&discord.client_secret))
            .send()
            .await?;
        let status = response.status();
        let body = response.bytes().await?;
        let token = serde_json::from_slice::<Token>(&body).map_err(|err| {
            match core::str::from_utf8(body.as_ref()) {
                Ok(v) => {
                    ::anyhow::Error::new(err).context(format!("Response Status code: {status}, Response Body: {v}"))
                },
                Err(err) => {
                    ::anyhow::Error::new(err).context(format!("Response Status code: {status}, Response Body: {body:?} ({err})"))
                },
            }
        })?;
        Ok(token)
    }
}

#[derive(Debug, Copy, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RevokeTokenType {
    AuthorizationCode,
    RefreshToken,
}
#[derive(Debug, Copy, Clone, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct RevokeTokenRequest<'a> {
    pub token: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_type: Option<RevokeTokenType>,
}
impl<'a> RevokeTokenRequest<'a> {
    pub const fn ambiguous_token(token: &'a str) -> Self {
        Self {
            token,
            token_type: None,
        }
    }

    pub const fn authorization_code(token: &'a str) -> Self {
        Self {
            token,
            token_type: Some(RevokeTokenType::AuthorizationCode),
        }
    }

    pub const fn refresh_token(token: &'a str) -> Self {
        Self {
            token,
            token_type: Some(RevokeTokenType::RefreshToken),
        }
    }

    pub async fn revoke_token(&self, discord: &super::Discord) -> ::anyhow::Result<()> {
        let response = discord.client.post("https://discord.com/api/oauth2/token/revoke")
            .form(self)
            .basic_auth(&discord.client_id, Some(&discord.client_secret))
            .send()
            .await?;
        if !response.status().is_success() && !response.status().is_informational() {
            let mut err = ::anyhow::Error::msg("Request status code indicated failure");
            let status = response.status();
            let body = response.bytes().await?;
            match core::str::from_utf8(body.as_ref()) {
                Ok(v) => {
                    err = err.context(format!("Response Status code: {status}, Response Body: {v}"));
                },
                Err(utf8_err) => {
                    err = err.context(format!("Response Status code: {status}, Response Body: {body:?} ({utf8_err})"));
                },
            }
            return Err(err);
        }
        Ok(())
    }
}