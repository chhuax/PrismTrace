use super::{ConsoleFilterContext, ConsoleSnapshot};

const CONSOLE_HTML_TEMPLATE: &str = include_str!("../../assets/console.html");

pub(crate) fn render_console_homepage(snapshot: &ConsoleSnapshot) -> String {
    let filter_context_html = render_filter_context_banner(snapshot.filter_context.as_ref());
    let theme_switcher_html = render_theme_switcher();
    let targets_html = String::new();
    let activity_html = String::new();
    let requests_html = String::new();
    let sessions_html = String::new();
    let session_timeline_html = String::new();
    let request_detail_html = String::new();
    let health_html = String::new();
    let initial_session_id = snapshot
        .session_summaries
        .first()
        .map(|session| escape_html(&session.session_id))
        .unwrap_or_default();
    let initial_request_id = snapshot
        .request_summaries
        .first()
        .map(|request| escape_html(&request.request_id))
        .unwrap_or_default();

    [
        ("__INITIAL_SESSION_ID__", initial_session_id.as_str()),
        ("__INITIAL_REQUEST_ID__", initial_request_id.as_str()),
        ("__SUMMARY__", snapshot.summary.as_str()),
        ("__BIND_ADDR__", snapshot.bind_addr.as_str()),
        ("__THEME_SWITCHER_HTML__", theme_switcher_html.as_str()),
        ("__FILTER_CONTEXT_HTML__", filter_context_html.as_str()),
        ("__TARGETS_HTML__", targets_html.as_str()),
        ("__ACTIVITY_HTML__", activity_html.as_str()),
        ("__SESSIONS_HTML__", sessions_html.as_str()),
        ("__REQUESTS_HTML__", requests_html.as_str()),
        ("__SESSION_TIMELINE_HTML__", session_timeline_html.as_str()),
        ("__REQUEST_DETAIL_HTML__", request_detail_html.as_str()),
        ("__HEALTH_HTML__", health_html.as_str()),
    ]
    .into_iter()
    .fold(
        CONSOLE_HTML_TEMPLATE.to_owned(),
        |html, (placeholder, value)| html.replace(placeholder, value),
    )
}

fn render_filter_context_banner(filter_context: Option<&ConsoleFilterContext>) -> String {
    let Some(filter_context) = filter_context else {
        return String::new();
    };

    let filters = filter_context
        .active_filters
        .iter()
        .map(|filter| {
            format!(
                "<span class=\"console-pill\">{}</span>",
                escape_html(filter)
            )
        })
        .collect::<Vec<_>>()
        .join("");

    format!(
        "<div class=\"console-entrypoint\"><p class=\"console-entrypoint-label\">Filtered monitor scope</p><div class=\"console-list-meta\">{}</div></div>",
        filters
    )
}

fn render_theme_switcher() -> String {
    let buttons = [
        ("system", "System"),
        ("dark", "Dark"),
        ("light", "Light"),
    ]
    .into_iter()
    .map(|(theme, label)| {
        format!(
            "<a class=\"console-pill\" href=\"?theme={theme}\" data-theme-switch=\"{theme}\">{label}</a>"
        )
    })
    .collect::<Vec<_>>()
    .join("");

    format!(
        "<div class=\"console-entrypoint console-theme-switch\"><p class=\"console-entrypoint-label\">Theme</p><div class=\"console-list-meta\">{buttons}</div></div>"
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
