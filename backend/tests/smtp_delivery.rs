//! Proves the SMTP sender actually delivers, rather than only compiling.
//!
//! Needs the catcher from `docker compose up -d mailpit`: SMTP on 1025, HTTP API
//! on 8025. Skipped when it is not running, so the suite still passes on a
//! machine that has not started it — but it fails loudly if Mailpit is up and
//! delivery is broken, which is the case worth catching.

use casivon_backend::config::{SmtpConfig, SmtpEncryption};
use casivon_backend::infrastructure::smtp_email::SmtpEmailSender;
use casivon_backend::shared::email::{EmailMessage, EmailSender};

const MAILPIT_API: &str = "http://127.0.0.1:8025/api/v1";

fn config() -> SmtpConfig {
    SmtpConfig {
        host: "127.0.0.1".to_string(),
        port: 1025,
        username: None,
        password: None,
        from: "ERP System <no-reply@erp.local>".to_string(),
        encryption: SmtpEncryption::None,
    }
}

/// Mailpit's API, via curl — the backend has no HTTP client dependency and this
/// is not worth adding one for.
fn mailpit(path: &str) -> Option<serde_json::Value> {
    let output = std::process::Command::new("curl")
        .args(["-s", "--max-time", "3", &format!("{MAILPIT_API}{path}")])
        .output()
        .ok()?;
    if !output.status.success() || output.stdout.is_empty() {
        return None;
    }
    serde_json::from_slice(&output.stdout).ok()
}

fn mailpit_running() -> bool {
    mailpit("/messages").is_some()
}

#[tokio::test]
async fn a_message_reaches_the_relay() {
    if !mailpit_running() {
        eprintln!("skipping: mailpit is not running (`docker compose up -d mailpit`)");
        return;
    }

    // A unique subject, so a parallel run or an old message cannot be mistaken
    // for this one.
    let marker = uuid::Uuid::new_v4();
    let subject = format!("Delivery probe {marker}");

    let sender = SmtpEmailSender::connect(&config()).expect("failed to build the transport");
    sender
        .send(EmailMessage {
            to: "ada@erp.test".to_string(),
            subject: subject.clone(),
            body: format!("Reset link: http://localhost:3000/reset-password?token={marker}"),
        })
        .await
        .expect("delivery failed");

    let messages = mailpit("/messages").expect("mailpit stopped responding");
    let delivered = messages["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["Subject"] == subject)
        .unwrap_or_else(|| panic!("no message with subject {subject} arrived"));

    assert_eq!(delivered["To"][0]["Address"], "ada@erp.test");
    assert_eq!(delivered["From"]["Address"], "no-reply@erp.local");
    assert_eq!(delivered["From"]["Name"], "ERP System");
}

#[tokio::test]
async fn non_ascii_text_survives_the_trip() {
    if !mailpit_running() {
        eprintln!("skipping: mailpit is not running");
        return;
    }

    let marker = uuid::Uuid::new_v4();
    let subject = format!("Encoding probe {marker}");
    // The reset template contains an em-dash, and real names contain accents.
    let body = "Hello Zoë — your password has not changed. Naïve café €10.";

    let sender = SmtpEmailSender::connect(&config()).unwrap();
    sender
        .send(EmailMessage {
            to: "ada@erp.test".to_string(),
            subject: subject.clone(),
            body: body.to_string(),
        })
        .await
        .expect("delivery failed");

    let messages = mailpit("/messages").expect("mailpit stopped responding");
    let id = messages["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["Subject"] == subject)
        .expect("the message did not arrive")["ID"]
        .as_str()
        .unwrap()
        .to_string();

    let delivered = mailpit(&format!("/message/{id}")).expect("could not read the message back");
    let text = delivered["Text"].as_str().unwrap();

    // Without an explicit `Content-Type: text/plain; charset=utf-8` the message
    // goes out untyped and these arrive as mojibake.
    assert!(text.contains("Zoë"), "got: {text}");
    assert!(text.contains("—"), "got: {text}");
    assert!(text.contains("café"), "got: {text}");
    assert!(text.contains("€10"), "got: {text}");
}

#[tokio::test]
async fn an_unroutable_address_is_reported_rather_than_swallowed() {
    if !mailpit_running() {
        eprintln!("skipping: mailpit is not running");
        return;
    }

    let sender = SmtpEmailSender::connect(&config()).unwrap();

    let result = sender
        .send(EmailMessage {
            to: "not-an-address".to_string(),
            subject: "Should not arrive".to_string(),
            body: "…".to_string(),
        })
        .await;

    assert!(result.is_err(), "a malformed recipient was accepted");
}

#[test]
fn a_malformed_from_address_fails_at_start_up() {
    // Better here, where someone is watching, than on the first password reset.
    let broken = SmtpConfig { from: "not an address".to_string(), ..config() };

    let error = match SmtpEmailSender::connect(&broken) {
        Ok(_) => panic!("a malformed SMTP_FROM was accepted"),
        Err(error) => error.to_string(),
    };
    assert!(error.contains("SMTP_FROM"), "{error}");
}
