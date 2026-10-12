use sacho::templates::{Templates, context};
use serde_json::json;
#[test]
fn incident_conditions_and_json_filters_render_without_secret_context() -> anyhow::Result<()> {
    let templates=Templates {
        subject:"{{ service_name }} · {{ status_name }}".into(),
        body:"{% if recovered %}Recovered{% else %}Incident{% endif %}: {{ service_name }} {{ display_value }}".into(),
        webhook_body:Some("{\"text\":{{ (service_name ~ ' ' ~ status_name) | tojson }},\"event\":{{ event | tojson }}}".into()),
        ..Default::default()
    };
    templates.validate()?;
    let payload = json!({"event":"incident.resolved","name":"A\"B","status":"operational","display_value":"42","unsubscribe_token":"secret-link-token","at":1700000000});
    let ctx = context("Example", "https://status.example.com", &payload);
    let (subject, body) = templates.email(&ctx)?;
    assert!(subject.contains("A\"B"));
    assert!(body.starts_with("Recovered: A\"B 42"));
    assert!(body.contains("/subscription/unsubscribe?token=secret-link-token"));
    assert_eq!(
        templates.webhook(&ctx, &payload)?["event"],
        "incident.resolved"
    );
    assert!(
        templates.webhook(&ctx, &payload)?["text"]
            .as_str()
            .unwrap()
            .starts_with("A\"B")
    );
    assert!(ctx.get("secret_values").is_none() && ctx.get("output").is_none());
    Ok(())
}
#[test]
fn missing_variables_invalid_json_subject_injection_and_size_are_rejected() {
    let templates = Templates {
        subject: "{{ unknown_variable }}".into(),
        ..Default::default()
    };
    assert!(templates.validate().is_err());
    let mut templates = Templates {
        webhook_body: Some("[]".into()),
        ..Default::default()
    };
    assert!(templates.validate().is_err());
    templates.webhook_body = Some("{bad}".into());
    assert!(templates.validate().is_err());
    let templates = Templates {
        subject: "Subject\nInjected: value".into(),
        ..Default::default()
    };
    assert!(templates.validate().is_err());
    let templates = Templates {
        body: "{{ 'x' * 70000 }}".into(),
        ..Default::default()
    };
    assert!(templates.validate().is_err());
    let templates = Templates {
        body: "{% for x in range(1000000) %}x{% endfor %}".into(),
        ..Default::default()
    };
    assert!(templates.validate().is_err());
}
#[test]
fn confirmation_links_survive_custom_body_and_defaults_preserve_webhook() -> anyhow::Result<()> {
    let templates = Templates {
        verification_body: "Please confirm.".into(),
        ..Default::default()
    };
    templates.validate()?;
    let payload = json!({"event":"subscription.verify","token":"recipient-token"});
    let ctx = context("Example", "https://status.example.com", &payload);
    assert!(
        templates
            .email(&ctx)?
            .1
            .contains("/subscription/confirm?token=recipient-token")
    );
    assert_eq!(templates.webhook(&ctx, &payload)?, payload);
    Ok(())
}

#[test]
fn warning_notification_is_not_labeled_as_an_outage() {
    let context = sacho::templates::context(
        "Site",
        "https://example.com",
        &serde_json::json!({"event":"incident.started","status":"degraded"}),
    );
    assert_eq!(context["event_name"], "Warning / 주의");
    assert_eq!(context["status_name"], "Warning / 주의");
}
