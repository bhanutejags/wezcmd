use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd")]
pub enum Command {
    #[serde(rename = "open")]
    Open(Open),
    #[serde(rename = "forward")]
    Forward(Forward),
    #[serde(rename = "vscode")]
    Vscode(Vscode),
    #[serde(rename = "notify")]
    Notify(Notify),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Open {
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Forward {
    pub port: Port,
    #[serde(default)]
    pub host: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vscode {
    pub path: String,
    #[serde(default)]
    pub host: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notify {
    #[serde(default = "default_title")]
    pub title: String,
    pub body: String,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Port(pub u16);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

impl Response {
    pub fn ok() -> Self {
        Self {
            ok: true,
            err: None,
        }
    }

    pub fn error(err: impl Into<String>) -> Self {
        Self {
            ok: false,
            err: Some(err.into().chars().take(120).collect()),
        }
    }
}

impl Command {
    pub fn from_json(input: &[u8]) -> Result<Self> {
        let cmd: Command = serde_json::from_slice(input).map_err(|_| anyhow!("invalid"))?;
        cmd.validate()?;
        Ok(cmd)
    }

    pub fn validate(&self) -> Result<()> {
        match self {
            Command::Open(Open { url }) => validate_url(url),
            Command::Forward(Forward { port: _, host }) => validate_host(host),
            Command::Vscode(Vscode { path, host }) => {
                validate_path(path)?;
                validate_host(host)
            }
            Command::Notify(Notify { title, body }) => {
                if title.len() > 200 {
                    bail!("invalid");
                }
                if body.is_empty() || body.len() > 2000 {
                    bail!("invalid");
                }
                Ok(())
            }
        }
    }
}

impl Serialize for Port {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u16(self.0)
    }
}

impl<'de> Deserialize<'de> for Port {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = Port;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a port number between 1024 and 65535")
            }

            fn visit_u64<E>(self, value: u64) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                let port = u16::try_from(value).map_err(|_| E::custom("invalid port"))?;
                validate_port(port).map_err(E::custom)?;
                Ok(Port(port))
            }

            fn visit_i64<E>(self, value: i64) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                let port = u16::try_from(value).map_err(|_| E::custom("invalid port"))?;
                validate_port(port).map_err(E::custom)?;
                Ok(Port(port))
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                let port = value
                    .parse::<u16>()
                    .map_err(|_| E::custom("invalid port"))?;
                validate_port(port).map_err(E::custom)?;
                Ok(Port(port))
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

fn default_title() -> String {
    "Notification".to_string()
}

pub fn validate_host(host: &str) -> Result<()> {
    if host.is_empty() {
        return Ok(());
    }
    let mut chars = host.chars();
    let Some(first) = chars.next() else {
        return Ok(());
    };
    if !first.is_ascii_alphanumeric() || host.len() > 255 {
        bail!("invalid");
    }
    if chars.any(|c| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '@' | '-'))) {
        bail!("invalid");
    }
    Ok(())
}

fn validate_path(path: &str) -> Result<()> {
    if !path.starts_with('/') || path.len() > 4096 || path.chars().any(|c| c.is_control()) {
        bail!("invalid");
    }
    Ok(())
}

fn validate_url(input: &str) -> Result<()> {
    let rest = input
        .strip_prefix("https://")
        .or_else(|| input.strip_prefix("http://"))
        .ok_or_else(|| anyhow!("invalid"))?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if host.is_empty() || input.chars().any(|c| c.is_control() || c.is_whitespace()) {
        bail!("invalid");
    }
    Ok(())
}

fn validate_port(port: u16) -> Result<()> {
    if port < 1024 {
        bail!("invalid");
    }
    Ok(())
}

#[cfg(test)]
fn validation_cases() -> &'static [(&'static str, bool, &'static str)] {
    &[
        (
            r#"{"cmd":"open","url":"https://example.com/x"}"#,
            true,
            "open https",
        ),
        (
            r#"{"cmd":"open","url":"http://example.com"}"#,
            true,
            "open http",
        ),
        (
            r#"{"cmd":"forward","port":8443}"#,
            true,
            "forward valid port",
        ),
        (
            r#"{"cmd":"forward","port":"9090"}"#,
            true,
            "forward numeric-string port",
        ),
        (
            r#"{"cmd":"open","url":"file:///etc/passwd"}"#,
            false,
            "reject file: scheme",
        ),
        (
            r#"{"cmd":"open","url":"javascript:alert(1)"}"#,
            false,
            "reject javascript:",
        ),
        (
            r#"{"cmd":"forward","port":80}"#,
            false,
            "reject port < 1024",
        ),
        (
            r#"{"cmd":"forward","port":70000}"#,
            false,
            "reject port > 65535",
        ),
        (
            r#"{"cmd":"forward","port":"notaport"}"#,
            false,
            "reject non-numeric port",
        ),
        (
            r#"{"cmd":"nuke","url":"https://x"}"#,
            false,
            "reject unknown verb",
        ),
        (
            r#"{"cmd":"open","url":"https://x","extra":"y"}"#,
            false,
            "reject extra field",
        ),
        (r#"{"cmd":"open"}"#, false, "reject missing url"),
        ("not json at all", false, "reject non-JSON"),
        (
            r#"{"cmd":"vscode","path":"/home/exedev/workplace/x"}"#,
            true,
            "vscode abs path",
        ),
        (
            r#"{"cmd":"vscode","path":"/x","host":"my-host"}"#,
            true,
            "vscode with host",
        ),
        (
            r#"{"cmd":"vscode","path":"/x","host":"bad host!"}"#,
            false,
            "reject bad host chars",
        ),
        (
            r#"{"cmd":"forward","port":8443,"host":"-oProxyCommand=x"}"#,
            false,
            "reject leading-dash host",
        ),
        (
            r#"{"cmd":"forward","port":8443,"host":""}"#,
            true,
            "empty host ok",
        ),
        (
            r#"{"cmd":"forward","port":8443,"host":"my-host"}"#,
            true,
            "forward with host",
        ),
        (
            r#"{"cmd":"vscode","path":"relative/x"}"#,
            false,
            "reject vscode relative path",
        ),
        (r#"{"cmd":"vscode"}"#, false, "reject vscode missing path"),
        (
            r#"{"cmd":"notify","title":"Build","body":"done"}"#,
            true,
            "notify title+body",
        ),
        (
            r#"{"cmd":"notify","body":"body only, default title"}"#,
            true,
            "notify default title",
        ),
        (
            r#"{"cmd":"notify","title":"x"}"#,
            false,
            "reject notify missing body",
        ),
        (
            r#"{"cmd":"notify","body":""}"#,
            false,
            "reject notify empty body",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_protocol_cases() {
        for (payload, accept, desc) in validation_cases() {
            let got = Command::from_json(payload.as_bytes()).is_ok();
            assert_eq!(got, *accept, "{desc}");
        }
    }

    #[test]
    fn serializes_wire_format() {
        let json = serde_json::to_string(&Command::Notify(Notify {
            title: "Build".into(),
            body: "done".into(),
        }))
        .unwrap();
        assert_eq!(json, r#"{"cmd":"notify","title":"Build","body":"done"}"#);
    }
}
