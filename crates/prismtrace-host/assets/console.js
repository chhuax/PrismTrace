const escapeHtml = (value) => String(value ?? '')
  .replaceAll('&', '&amp;')
  .replaceAll('<', '&lt;')
  .replaceAll('>', '&gt;')
  .replaceAll('"', '&quot;')
  .replaceAll("'", '&#39;');

const readThemePreference = () => {
  const url = new URL(window.location.href);
  const theme = url.searchParams.get('theme');
  if (theme === 'dark' || theme === 'light' || theme === 'system') {
    window.localStorage.setItem('prismtrace-console-theme', theme);
    return theme;
  }

  const stored = window.localStorage.getItem('prismtrace-console-theme');
  if (stored === 'dark' || stored === 'light' || stored === 'system') {
    return stored;
  }

  return 'system';
};

const applyThemePreference = (theme) => {
  document.body.dataset.theme = theme;
  document.querySelectorAll('[data-theme-switch]').forEach((item) => {
    item.classList.toggle('is-selected', item.getAttribute('data-theme-switch') === theme);
  });
};

const renderEmptyState = (text) => `<p class="muted console-placeholder">${escapeHtml(text)}</p>`;

const renderTargets = (payload) => {
  if (!payload.targets?.length) return renderEmptyState(payload.empty_state || '尚无可观测目标');
  return `<div class="console-list">${payload.targets.map((target) => `
    <article class="console-list-item">
      <p class="console-list-title">${escapeHtml(target.display_name)}</p>
      <p class="console-list-subtitle">PID ${escapeHtml(target.pid)} · ${escapeHtml(target.runtime_kind)}</p>
      <div class="console-list-meta">
        <span class="console-pill">attach: ${escapeHtml(target.attach_state)}</span>
        <span class="console-pill">${escapeHtml(target.probe_state_summary)}</span>
      </div>
    </article>`).join('')}</div>`;
};

const renderActivity = (payload) => {
  if (!payload.activity?.length) return renderEmptyState(payload.empty_state || '尚无观测活动');
  return `<div class="console-list">${payload.activity.map((item) => `
    <article class="console-list-item">
      <p class="console-list-title">${escapeHtml(item.title)}</p>
      <p class="console-list-subtitle">${escapeHtml(item.subtitle)}</p>
      <div class="console-list-meta">
        <span class="console-pill">${escapeHtml(item.activity_type)}</span>
        <span class="console-pill">ts: ${escapeHtml(item.occurred_at_ms)}</span>
      </div>
    </article>`).join('')}</div>`;
};

const renderRequests = (payload) => {
  if (!payload.requests?.length) return renderEmptyState(payload.empty_state || '尚无请求记录');
  return `<div class="console-list console-request-stream">${payload.requests.map((request) => `
    <article class="console-list-item is-actionable console-request-stream-item" data-request-id="${escapeHtml(request.request_id)}" data-request-detail-trigger="${escapeHtml(request.request_id)}" tabindex="0" role="button" aria-label="view request detail for ${escapeHtml(request.summary_text)}">
      <div class="console-request-stream-top">
        <p class="console-request-stream-kicker">ts ${escapeHtml(request.captured_at_ms)}</p>
        <div class="console-request-stream-main">
          <p class="console-list-title">${escapeHtml(request.summary_text)}</p>
          <div class="console-request-stream-route">
            <span class="console-request-stream-method">POST</span>
            <span class="console-request-stream-path">${escapeHtml(request.target_display_name)}</span>
          </div>
        </div>
      </div>
      <div class="console-list-meta">
        <span class="console-pill">provider: ${escapeHtml(request.provider)}</span>
        <span class="console-pill">model: ${escapeHtml(request.model || 'unknown')}</span>
        <button type="button" class="console-pill" data-request-detail-trigger="${escapeHtml(request.request_id)}">view detail</button>
      </div>
    </article>`).join('')}</div>`;
};

const renderSessions = (payload) => {
  if (!payload.sessions?.length) return renderEmptyState(payload.empty_state || '尚无会话记录');
  return `<div class="console-list">${payload.sessions.map((session) => `
    <article class="console-list-item is-actionable" data-session-id="${escapeHtml(session.session_id)}" data-session-detail-trigger="${escapeHtml(session.session_id)}" tabindex="0" role="button" aria-label="view session timeline for ${escapeHtml(session.target_display_name)}">
      <p class="console-list-title">${escapeHtml(session.target_display_name)}</p>
      <p class="console-list-subtitle">PID ${escapeHtml(session.pid)} · ${escapeHtml(session.started_at_ms)} → ${escapeHtml(session.completed_at_ms)}</p>
      <div class="console-list-meta">
        <span class="console-pill">exchanges: ${escapeHtml(session.exchange_count)}</span>
        <span class="console-pill">responses: ${escapeHtml(session.response_count)}</span>
        <button type="button" class="console-pill" data-session-detail-trigger="${escapeHtml(session.session_id)}">view timeline</button>
      </div>
    </article>`).join('')}</div>`;
};

const renderHeaderList = (headers, emptyText) => {
  if (!headers?.length) return renderEmptyState(emptyText);
  return `<div class="console-list">${headers.map((header) => `
    <article class="console-list-item">
      <p class="console-list-title">${escapeHtml(header.name)}</p>
      <p class="console-list-subtitle"><code>${escapeHtml(header.value)}</code></p>
    </article>`).join('')}</div>`;
};

const renderBodyBlock = (bodyText, truncated, emptyText) => {
  if (!bodyText) return renderEmptyState(emptyText);
  const truncatedLabel = truncated
    ? '<p class="console-detail-label">captured body is truncated</p>'
    : '';
  return `${truncatedLabel}<pre class="console-code-block">${escapeHtml(bodyText)}</pre>`;
};

const renderToolList = (tools, emptyText) => {
  if (!tools?.length) return renderEmptyState(emptyText);
  return `<div class="console-list">${tools.map((tool) => `
    <article class="console-list-item">
      <p class="console-list-title">${escapeHtml(tool.name)}</p>
      <p class="console-list-subtitle">type: ${escapeHtml(tool.tool_type)}</p>
    </article>`).join('')}</div>`;
};

const renderRequestDetail = (payload) => {
  const request = payload.request;
  if (!request || request.status === 'not_found') {
    return renderEmptyState(request?.detail || 'request detail is not available yet');
  }

  const response = request.response;
  const toolVisibility = request.tool_visibility;
  return `<div class="console-detail-grid console-detail-grid-inspector">
    <section class="console-detail-section">
      <p class="console-detail-section-title">Request Overview</p>
      <div class="console-detail-row">
        <p class="console-detail-label">Request Summary</p>
        <p class="console-list-title">${escapeHtml(request.request_summary)}</p>
      </div>
      <div class="console-detail-row">
        <p class="console-detail-label">Target</p>
        <p>${escapeHtml(request.target_display_name)}</p>
      </div>
      <div class="console-detail-row">
        <p class="console-detail-label">Provider / Model</p>
        <p>${escapeHtml(request.provider)} · ${escapeHtml(request.model || 'unknown')}</p>
      </div>
      <div class="console-detail-row">
        <p class="console-detail-label">Request Route</p>
        <p><code>${escapeHtml(request.method)} ${escapeHtml(request.url)}</code></p>
      </div>
      <div class="console-detail-row">
        <p class="console-detail-label">Exchange / Hook</p>
        <p>${escapeHtml(request.exchange_id || 'unknown')} · ${escapeHtml(request.hook_name || 'unknown')}</p>
      </div>
      <div class="console-detail-row">
        <p class="console-detail-label">Artifact Path</p>
        <p><code>${escapeHtml(request.artifact_path)}</code></p>
      </div>
      <div class="console-detail-row">
        <p class="console-detail-label">Probe Context</p>
        <p>${escapeHtml(request.probe_context || '暂无 probe context')}</p>
      </div>
    </section>
    <section class="console-detail-section">
      <p class="console-detail-section-title">Request Payload</p>
      <div class="console-detail-row">
        <p class="console-detail-label">Headers</p>
        ${renderHeaderList(request.headers, '未记录 request headers')}
      </div>
      <div class="console-detail-row">
        <p class="console-detail-label">Body (${escapeHtml(request.body_size_bytes || 0)} bytes)</p>
        ${renderBodyBlock(request.body_text, request.truncated, '未记录 request body')}
      </div>
    </section>
    <section class="console-detail-section">
      <p class="console-detail-section-title">Tool Visibility</p>
      ${toolVisibility ? `
        <div class="console-detail-row">
          <p class="console-detail-label">Stage / Count</p>
          <p class="console-list-title">${escapeHtml(toolVisibility.visibility_stage)} · ${escapeHtml(toolVisibility.tool_count_final)} tool(s)</p>
        </div>
        <div class="console-detail-row">
          <p class="console-detail-label">Tool Choice</p>
          <p><code>${escapeHtml(toolVisibility.tool_choice || '未记录 tool choice')}</code></p>
        </div>
        <div class="console-detail-row">
          <p class="console-detail-label">Final Tools</p>
          ${renderToolList(toolVisibility.final_tools, 'final tools array is empty')}
        </div>
        <div class="console-detail-row">
          <p class="console-detail-label">Visibility Artifact</p>
          <p><code>${escapeHtml(toolVisibility.artifact_path)}</code></p>
        </div>
        <div class="console-detail-row">
          <p class="console-detail-label">Final Tools JSON</p>
          ${renderBodyBlock(toolVisibility.final_tools_json, false, '未记录 final tools json')}
        </div>
      ` : renderEmptyState('尚未关联到 tool visibility artifact')}
    </section>
    <section class="console-detail-section">
      <p class="console-detail-section-title">Response Detail</p>
      ${response ? `
        <div class="console-detail-row">
          <p class="console-detail-label">Status / Duration</p>
          <p class="console-list-title">${escapeHtml(response.status_code)} · ${escapeHtml(response.duration_ms)}ms</p>
        </div>
        <div class="console-detail-row">
          <p class="console-detail-label">Response Timing</p>
          <p>${escapeHtml(response.started_at_ms)} → ${escapeHtml(response.completed_at_ms)}</p>
        </div>
        <div class="console-detail-row">
          <p class="console-detail-label">Response Artifact</p>
          <p><code>${escapeHtml(response.artifact_path)}</code></p>
        </div>
        <div class="console-detail-row">
          <p class="console-detail-label">Headers</p>
          ${renderHeaderList(response.headers, '未记录 response headers')}
        </div>
        <div class="console-detail-row">
          <p class="console-detail-label">Body (${escapeHtml(response.body_size_bytes || 0)} bytes)</p>
          ${renderBodyBlock(response.body_text, response.truncated, '尚未记录 response body')}
        </div>
      ` : renderEmptyState('尚未关联到 response artifact')}
    </section>
  </div>`;
};

const renderSessionDetail = (payload) => {
  const session = payload.session;
  if (!session || session.status === 'not_found') {
    return renderEmptyState(session?.detail || 'session detail is not available yet');
  }

  if (!session.timeline_items?.length) {
    return renderEmptyState('当前 session 尚无 timeline item');
  }

  return `<div class="console-detail-grid">
    <section class="console-detail-section">
      <p class="console-detail-section-title">Session Overview</p>
      <div class="console-detail-row">
        <p class="console-detail-label">Session</p>
        <p class="console-list-title">${escapeHtml(session.target_display_name)} · PID ${escapeHtml(session.pid)}</p>
      </div>
      <div class="console-detail-row">
        <p class="console-detail-label">Window</p>
        <p>${escapeHtml(session.started_at_ms)} → ${escapeHtml(session.completed_at_ms)}</p>
      </div>
      <div class="console-detail-row">
        <p class="console-detail-label">Exchange Count</p>
        <p>${escapeHtml(session.exchange_count)}</p>
      </div>
    </section>
    <section class="console-detail-section">
      <p class="console-detail-section-title">Timeline</p>
      <div class="console-list console-timeline-list">${session.timeline_items.map((item) => `
        <article class="console-list-item is-actionable console-timeline-item" data-request-detail-trigger="${escapeHtml(item.request_id)}" tabindex="0" role="button" aria-label="view request detail for ${escapeHtml(item.request_summary)}">
          <p class="console-list-title">${escapeHtml(item.request_summary)}</p>
          <p class="console-list-subtitle">${escapeHtml(item.started_at_ms)} → ${escapeHtml(item.completed_at_ms)} · ${escapeHtml(item.target_display_name)}</p>
          <div class="console-list-meta">
            <span class="console-pill">provider: ${escapeHtml(item.provider)}</span>
            <span class="console-pill">model: ${escapeHtml(item.model || 'unknown')}</span>
            <span class="console-pill">status: ${escapeHtml(item.response_status ?? 'pending')}</span>
            <span class="console-pill">tools: ${escapeHtml(item.tool_count_final)}</span>
            <button type="button" class="console-pill" data-request-detail-trigger="${escapeHtml(item.request_id)}">view request</button>
          </div>
        </article>`).join('')}</div>
    </section>
  </div>`;
};

const renderHealth = (payload) => {
  const cards = [];

  if (payload.probe_summary) {
    cards.push(`<article class="console-health-card"><p class="console-detail-label">Probe Summary</p><p class="console-list-title">${escapeHtml(payload.probe_summary)}</p></article>`);
  }

  if (payload.errors?.length) {
    cards.push(...payload.errors.map((error) => `
      <article class="console-health-card is-error">
        <p class="console-detail-label">${escapeHtml(error.title)}</p>
        <p class="console-list-title">${escapeHtml(error.subtitle)}</p>
      </article>`));
  }

  if (!cards.length) {
    return renderEmptyState(payload.empty_state || '尚未发现 probe 健康或错误提示');
  }

  return `<div class="console-health-stack">${cards.join('')}</div>`;
};

const refreshRegion = async (endpoint, regionId, render) => {
  const region = document.getElementById(regionId);
  if (!region) return;
  try {
    const response = await fetch(endpoint);
    if (!response.ok) throw new Error(`request failed: ${response.status}`);
    const payload = await response.json();
    region.innerHTML = render(payload);
  } catch (error) {
    region.innerHTML = renderEmptyState(`加载失败：${error.message}`);
  }
};

const refreshRequestDetail = async (requestId) => {
  const region = document.getElementById('request-detail-region');
  if (!region) return;
  if (!requestId) {
    region.innerHTML = renderEmptyState('请选择一条 request 查看基础详情');
    return;
  }

  await refreshRegion(`/api/requests/${requestId}`, 'request-detail-region', renderRequestDetail);
};

const refreshSessionDetail = async (sessionId) => {
  const region = document.getElementById('session-detail-region');
  if (!region) return;
  if (!sessionId) {
    region.innerHTML = renderEmptyState('请选择一个 session 查看 timeline');
    return;
  }

  await refreshRegion(`/api/sessions/${sessionId}`, 'session-detail-region', renderSessionDetail);
};

document.addEventListener('click', (event) => {
  const trigger = event.target.closest('[data-request-detail-trigger]');
  if (trigger) {
    void refreshRequestDetail(trigger.getAttribute('data-request-detail-trigger'));
    return;
  }

  const sessionTrigger = event.target.closest('[data-session-detail-trigger]');
  if (!sessionTrigger) return;
  void refreshSessionDetail(sessionTrigger.getAttribute('data-session-detail-trigger'));
});

document.addEventListener('keydown', (event) => {
  if (event.key !== 'Enter' && event.key !== ' ') return;
  const trigger = event.target.closest('[data-request-detail-trigger], [data-session-detail-trigger]');
  if (!trigger) return;
  event.preventDefault();
  trigger.click();
});

applyThemePreference(readThemePreference());

void refreshRegion('/api/targets', 'targets-region', renderTargets);
void refreshRegion('/api/activity', 'activity-region', renderActivity);
void refreshRegion('/api/requests', 'requests-region', renderRequests);
void refreshRegion('/api/sessions', 'sessions-region', renderSessions);
void refreshRegion('/api/health', 'health-region', renderHealth);

const initialSessionId = document.body.dataset.initialSessionId || null;
const initialRequestId = document.body.dataset.initialRequestId || null;

void refreshSessionDetail(initialSessionId);
void refreshRequestDetail(initialRequestId);
