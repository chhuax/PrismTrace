const escapeHtml = (value) =>
  String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");

const DEFAULT_LANGUAGE = "zh-CN";
const SESSION_PAGE_LIMIT = 30;
const SESSION_EVENT_PAGE_LIMIT = 100;
const LANGUAGE_ALIASES = {
  zh: "zh-CN",
  "zh-CN": "zh-CN",
  en: "en-US",
  "en-US": "en-US"
};

const state = {
  language: DEFAULT_LANGUAGE,
  translations: {},
  theme: "light",
  sessions: [],
  sessionNextCursor: null,
  sessionLoadingMore: false,
  sessionDetail: null,
  sessionCapabilities: [],
  sessionDiagnostics: null,
  sessionEventsNextCursor: null,
  sessionEventsLoadingMore: false,
  selectedSessionId: null,
  selectedRuntime: "all",
  runtimeSearch: "",
  globalSearch: "",
  timelineKeyword: "",
  selectedOnly: false,
  selectedItemIds: new Set(),
  detailCache: new Map(),
  leftCollapsed: false,
  rightCollapsed: false
};

const translationCache = {};

const ICON_FALLBACKS = {
  add: "+",
  arrow_drop_down: "v",
  arrow_forward: "->",
  attach_file: "clip",
  build: "tool",
  check: "ok",
  chevron_right: ">",
  data_object: "{}",
  deployed_code: "cap",
  description: "doc",
  difference: "diff",
  edit: "edit",
  expand_less: "^",
  expand_more: "v",
  filter_list: "filter",
  forum: "chat",
  functions: "fn",
  more_vert: "...",
  north: "^",
  output: "out",
  person: "user",
  psychology: "skill",
  refresh: "reload",
  rule: "rule",
  settings: "cfg",
  smart_toy: "AI",
  south: "v",
  sync: "sync",
  terminal: ">_",
  tune: "tune"
};

const hydrateLocalIcons = (root = document) => {
  root.querySelectorAll(".material-symbols-outlined").forEach((node) => {
    const original = node.dataset.iconName || node.textContent.trim();
    if (!original) return;
    node.dataset.iconName = original;
    node.textContent = ICON_FALLBACKS[original] || original;
    node.setAttribute("aria-hidden", "true");
  });
};

const observeLocalIcons = () => {
  hydrateLocalIcons();
  const observer = new MutationObserver((mutations) => {
    mutations.forEach((mutation) => {
      mutation.addedNodes.forEach((node) => {
        if (node.nodeType !== Node.ELEMENT_NODE) return;
        if (node.classList?.contains("material-symbols-outlined")) {
          hydrateLocalIcons(node.parentElement || document);
        } else {
          hydrateLocalIcons(node);
        }
      });
    });
  });
  observer.observe(document.body, { childList: true, subtree: true });
};

const normalizeLanguage = (value) => LANGUAGE_ALIASES[String(value || "").trim()] || DEFAULT_LANGUAGE;

const t = (key, fallback) => state.translations[key] || fallback;

const readLanguagePreference = () => {
  const url = new URL(window.location.href);
  const queryLanguage = url.searchParams.get("lang");
  if (queryLanguage) {
    const normalized = normalizeLanguage(queryLanguage);
    window.localStorage.setItem("prismtrace-console-language", normalized);
    return normalized;
  }
  return normalizeLanguage(window.localStorage.getItem("prismtrace-console-language"));
};

const loadLanguageResource = async (language) => {
  if (translationCache[language]) return translationCache[language];
  const response = await fetch(`/assets/i18n/${language}.json`);
  if (!response.ok) throw new Error(`language resource unavailable: ${language}`);
  const payload = await response.json();
  translationCache[language] = payload;
  return payload;
};

const readThemePreference = () => {
  const url = new URL(window.location.href);
  const queryTheme = url.searchParams.get("theme");
  if (queryTheme === "dark" || queryTheme === "light") {
    window.localStorage.setItem("prismtrace-console-theme", queryTheme);
    return queryTheme;
  }
  const stored = window.localStorage.getItem("prismtrace-console-theme");
  return stored === "dark" ? "dark" : "light";
};

const applyTheme = (theme) => {
  state.theme = theme === "light" ? "light" : "dark";
  document.documentElement.classList.toggle("dark", state.theme === "dark");
  document.body.dataset.theme = state.theme;
  const stylesheet = document.getElementById("theme-stylesheet");
  if (stylesheet) {
    stylesheet.href = state.theme === "light" ? "/assets/console-theme-light.css" : "/assets/console-theme-dark.css";
  }
};

const applyStaticTranslations = () => {
  document.documentElement.lang = state.language === "zh-CN" ? "zh-CN" : "en";
  document.querySelectorAll("[data-i18n]").forEach((node) => {
    const key = node.getAttribute("data-i18n");
    const fallback = node.dataset.i18nFallback || node.textContent.trim();
    node.dataset.i18nFallback = fallback;
    node.textContent = t(key, fallback);
  });

  document.querySelectorAll("[data-i18n-placeholder]").forEach((node) => {
    const key = node.getAttribute("data-i18n-placeholder");
    const fallback = node.dataset.i18nPlaceholderFallback || node.getAttribute("placeholder") || "";
    node.dataset.i18nPlaceholderFallback = fallback;
    node.setAttribute("placeholder", t(key, fallback));
  });
};

const formatTime = (value) => {
  const numeric = Number(value);
  if (!Number.isFinite(numeric) || numeric <= 0) return "--:--:--";
  return new Date(numeric).toLocaleTimeString([], {
    hour12: false,
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit"
  });
};

const formatRelativeTime = (value) => {
  const numeric = Number(value);
  if (!Number.isFinite(numeric) || numeric <= 0) return "";
  const diffMs = Math.max(0, Date.now() - numeric);
  const minutes = Math.floor(diffMs / 60000);
  if (minutes < 1) return t("time.just_now", "刚刚");
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  return `${Math.floor(hours / 24)}d`;
};

const shortenPath = (value) => {
  const text = String(value || "").trim();
  if (!text) return "";
  return text.replace(/^\/Volumes\/MacData\/workspace\//, "").replace(/^\/Users\/[^/]+\/\.codex\/sessions\//, "~/.codex/sessions/");
};

const displayProjectPath = (value) => {
  const text = String(value || "").trim();
  if (!text) return t("sessions.unknown_project", "未知项目路径");
  return text;
};

const inferRuntime = (item = {}) => {
  const id = String(item.session_id || item.request_id || "");
  const source = `${item.target_display_name || ""} ${item.provider || ""}`.toLowerCase();
  if (id.startsWith("codex-thread:") || id.includes(":codex:") || source.includes("codex")) return "codex";
  if (id.includes("opencode") || source.includes("opencode")) return "opencode";
  if (id.includes("claude") || source.includes("claude")) return "claude";
  return "observer";
};

const runtimeLabel = (runtime) => {
  if (runtime === "codex") return "Codex Desktop";
  if (runtime === "opencode") return "opencode";
  if (runtime === "claude") return "Claude Code";
  if (runtime === "observer") return "Custom observer";
  return t("runtime.all", "All runtimes");
};

const projectLabel = (session) => {
  const cwd = String(session.cwd || "").trim();
  if (cwd) return cwd;
  const artifactPath = String(session.artifact_path || "").trim();
  if (artifactPath) return artifactPath;
  return String(session.target_display_name || "SESSIONS").trim();
};

const isArchivedSession = (session) => {
  const artifactPath = String(session.artifact_path || "").toLowerCase();
  return (
    artifactPath.includes("/archived_sessions/") ||
    artifactPath.includes("\\archived_sessions\\") ||
    session.archived === true
  );
};

const sessionTitle = (session) =>
  String(session?.title || "").trim() ||
  String(session?.session_id || "").replace(/^codex-thread:/, "codex:");

const sessionSubtitle = (session) =>
  String(session?.subtitle || "").trim() ||
  `${session?.exchange_count || 0} ${t("sessions.events", "events")}`;

const parseTimelineRole = (item) => {
  const model = String(item.model || "").toLowerCase();
  const summary = String(item.request_summary || "");
  if (model === "message") {
    if (/^(用户|User):/i.test(summary)) return "user";
    if (/^(助手|Assistant|Codex):/i.test(summary)) return "assistant";
    if (/^(系统|System):/i.test(summary)) return "system";
  }
  if (model === "instruction") return "instruction";
  if (model === "workspace_context") return "context";
  if (model === "tool" || model === "tool_call") return "tool";
  if (model === "tool_result") return "result";
  if (["agent", "app", "mcp", "plugin", "provider", "skill"].includes(model)) return "capability";
  return "event";
};

const roleCopy = (role) => {
  const zh = state.language === "zh-CN";
  const copy = {
    user: zh ? "用户" : "User",
    assistant: "Codex",
    system: zh ? "系统" : "System",
    instruction: zh ? "指令" : "Instruction",
    context: zh ? "上下文" : "Context",
    tool: zh ? "工具调用" : "Tool Call",
    result: zh ? "工具结果" : "Result",
    capability: zh ? "能力快照" : "Capability",
    event: zh ? "事件" : "Event"
  };
  return copy[role] || copy.event;
};

const roleIcon = (role) => {
  const icons = {
    user: "person",
    assistant: "smart_toy",
    system: "settings",
    instruction: "rule",
    context: "description",
    tool: "build",
    result: "output",
    capability: "deployed_code",
    event: "radio_button_checked"
  };
  return icons[role] || icons.event;
};

const roleColorClass = (role) => {
  if (role === "assistant") return "text-primary";
  if (role === "tool") return "text-secondary";
  if (role === "result" || role === "context" || role === "instruction") return "text-on-surface-variant";
  if (role === "capability") return "text-tertiary";
  return "text-on-surface";
};

const timelineIconShellClass = (role) => {
  if (role === "assistant") return "bg-white border border-primary/30 text-primary shadow-sm";
  if (role === "tool") return "bg-secondary-container border border-secondary/30 text-secondary";
  if (role === "capability") return "bg-primary-container border border-primary/30 text-primary";
  return "bg-surface-container-highest border border-outline-variant text-on-surface-variant";
};

const cleanSummary = (item) => {
  const role = parseTimelineRole(item);
  let text = String(item.request_summary || "");
  if (role === "user") text = text.replace(/^用户:\s*/i, "").replace(/^User:\s*/i, "");
  if (role === "assistant") text = text.replace(/^助手:\s*/i, "").replace(/^Assistant:\s*/i, "").replace(/^Codex:\s*/i, "");
  if (role === "system") text = text.replace(/^系统:\s*/i, "").replace(/^System:\s*/i, "");
  if (role === "instruction") text = text.replace(/^开发者指令:\s*/i, "").replace(/^Instruction:\s*/i, "");
  if (role === "context") text = text.replace(/^工作区上下文:\s*/i, "").replace(/^Workspace context:\s*/i, "");
  return text.trim() || String(item.request_id || "");
};

const apiJson = async (path) => {
  const separator = path.includes("?") ? "&" : "?";
  const response = await fetch(`${path}${separator}t=${Date.now()}`, { cache: "no-store" });
  if (!response.ok) throw new Error(`${path} returned ${response.status}`);
  return response.json();
};

const detailApiPathForRequestId = (requestId) => {
  if (requestId.startsWith("observer:") || requestId.startsWith("codex-thread:")) {
    return `/api/events/${requestId}`;
  }
  return `/api/requests/${requestId}`;
};

const groupBy = (items, keyFn) =>
  items.reduce((groups, item) => {
    const key = keyFn(item);
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(item);
    return groups;
  }, new Map());

const appendUniqueById = (items, incoming, idKey) => {
  const seen = new Set(items.map((item) => item?.[idKey]).filter(Boolean));
  incoming.forEach((item) => {
    const id = item?.[idKey];
    if (!id || seen.has(id)) return;
    seen.add(id);
    items.push(item);
  });
  return items;
};

const filteredSessions = () => {
  const globalNeedle = state.globalSearch.trim().toLowerCase();
  const runtimeNeedle = state.runtimeSearch.trim().toLowerCase();
  return state.sessions.filter((session) => {
    if (isArchivedSession(session)) return false;
    const runtime = inferRuntime(session);
    if (state.selectedRuntime !== "all" && runtime !== state.selectedRuntime) return false;
    const haystack = `${sessionTitle(session)} ${sessionSubtitle(session)} ${session.session_id || ""} ${session.target_display_name || ""} ${projectLabel(session)}`.toLowerCase();
    if (globalNeedle && !haystack.includes(globalNeedle)) return false;
    if (runtimeNeedle && !runtimeLabel(runtime).toLowerCase().includes(runtimeNeedle)) return false;
    return true;
  });
};

const renderRuntimeList = () => {
  const region = document.getElementById("runtime-list-region");
  if (!region) return;
  const counts = state.sessions.filter((session) => !isArchivedSession(session)).reduce(
    (acc, session) => {
      const runtime = inferRuntime(session);
      acc.all += 1;
      acc[runtime] = (acc[runtime] || 0) + 1;
      return acc;
    },
    { all: 0 }
  );
  const runtimes = ["all", "codex", "opencode", "claude", "observer"];
  region.innerHTML = runtimes
    .filter((runtime) => runtime === "all" || counts[runtime])
    .map((runtime) => {
      const active = state.selectedRuntime === runtime;
      return `
        <li>
          <button class="w-full px-2 py-1.5 rounded ${
            active ? "bg-primary-container text-primary font-semibold" : "hover:bg-surface-container-high text-on-surface-variant"
          } cursor-pointer flex justify-between items-center text-left" data-runtime-value="${escapeHtml(runtime)}" type="button">
            <span>${escapeHtml(runtimeLabel(runtime))}</span>
            <span class="flex items-center gap-1">
              <span>${counts[runtime] || 0}</span>
              ${active ? '<span class="material-symbols-outlined text-[14px]">check</span>' : ""}
            </span>
          </button>
        </li>
      `;
    })
    .join("");
};

const renderSessionList = () => {
  const region = document.getElementById("session-list-region");
  if (!region) return;
  const sessions = filteredSessions();
  if (!sessions.length) {
    region.innerHTML = `<div class="p-4 text-[11px] normal-case tracking-normal text-on-surface-variant">${escapeHtml(
      t("state.no_sessions_available", "暂无可用会话")
    )}</div>`;
    return;
  }

  const groups = groupBy(sessions, projectLabel);
  const groupsHtml = Array.from(groups.entries())
    .map(([project, groupSessions]) => {
      const projectPath = displayProjectPath(project);
      return `
      <div class="px-3 py-1 mb-1 mt-2" title="${escapeHtml(projectPath)}">
        <div class="text-[10px] font-bold text-on-surface-variant/60 uppercase tracking-widest truncate">${escapeHtml(projectPath)}</div>
      </div>
      <div class="space-y-0.5 px-2 mb-4">
        ${groupSessions.map(renderSessionRow).join("")}
      </div>
    `;
    })
    .join("");
  const hasMore = state.sessionNextCursor !== null && state.sessionNextCursor !== undefined;
  const loadMore = hasMore
    ? `
      <div class="px-2 py-2">
        <button class="w-full px-2.5 py-1.5 rounded-sm bg-surface-container-high text-on-surface text-[11px] font-medium hover:bg-outline-variant transition-colors border border-outline-variant" data-load-more-sessions type="button">
          ${escapeHtml(state.sessionLoadingMore ? t("state.loading_sessions", "加载会话...") : t("action.load_more", "加载更多"))}
        </button>
      </div>
    `
    : "";
  region.innerHTML = `${groupsHtml}${loadMore}`;
};

const renderSessionRow = (session) => {
  const active = session.session_id === state.selectedSessionId;
  const runtime = inferRuntime(session);
  return `
    <button class="group relative w-full text-left ${
      active
        ? "bg-surface-container-high rounded cursor-pointer border border-primary/20 text-primary"
        : "hover:bg-surface-container-high transition-all duration-200 rounded cursor-pointer text-on-surface-variant"
    }" data-session-id="${escapeHtml(session.session_id)}" type="button">
      ${active ? '<span class="absolute left-0 top-0 bottom-0 w-0.5 bg-primary rounded-l"></span>' : ""}
      <span class="p-2 normal-case flex gap-2">
        <span class="material-symbols-outlined text-[14px] mt-0.5 opacity-80">terminal</span>
        <span class="flex-1 min-w-0">
          <span class="font-mono text-[10px] ${active ? "font-bold text-primary" : ""} truncate mb-0.5 block">[${escapeHtml(runtimeLabel(runtime).replace(" Desktop", ""))}] ${escapeHtml(sessionTitle(session))}</span>
          <span class="text-[9px] ${active ? "text-on-surface-variant" : "opacity-60"} tracking-normal block">${escapeHtml(formatRelativeTime(session.completed_at_ms) || "Active")} · ${escapeHtml(sessionSubtitle(session))}</span>
        </span>
      </span>
    </button>
  `;
};

const currentSessionSummary = () => state.sessions.find((session) => session.session_id === state.selectedSessionId) || null;

const renderSessionHeader = () => {
  const titleRegion = document.getElementById("session-title-region");
  const metaRegion = document.getElementById("session-meta-region");
  const summary = currentSessionSummary();
  const detail = state.sessionDetail?.session;
  if (!titleRegion || !metaRegion) return;

  if (!summary) {
    titleRegion.textContent = t("state.no_session_selected", "选择一个会话");
    metaRegion.innerHTML = `<span>${escapeHtml(
      state.language === "zh-CN" ? "Live monitoring across all observed sources." : "Live monitoring across all observed sources."
    )}</span>`;
    return;
  }

  const runtime = runtimeLabel(inferRuntime(summary));
  const cwd = detail?.cwd || summary.cwd || detail?.artifact_path || "";
  titleRegion.textContent = sessionTitle(summary);
  const inspectorTimeRegion = document.getElementById("inspector-time-region");
  if (inspectorTimeRegion) inspectorTimeRegion.textContent = formatTime(detail?.completed_at_ms || summary.completed_at_ms);
  metaRegion.innerHTML = `
    <span class="text-primary">${escapeHtml(runtime)}</span>
    <span class="opacity-50">·</span>
    <span class="truncate max-w-[360px]" title="${escapeHtml(cwd)}">${escapeHtml(shortenPath(cwd) || summary.target_display_name || "")}</span>
    <span class="opacity-50">·</span>
    <span>${escapeHtml(String(detail?.exchange_count ?? summary.exchange_count ?? 0))} ${escapeHtml(t("sessions.events", "events"))}</span>
  `;
};

const filteredTimelineItems = () => {
  const items = state.sessionDetail?.session?.timeline_items || [];
  const needle = state.timelineKeyword.trim().toLowerCase();
  return items.filter((item) => {
    if (state.selectedOnly && !state.selectedItemIds.has(item.request_id)) return false;
    if (!needle) return true;
    return `${item.request_summary || ""} ${item.model || ""} ${item.provider || ""} ${item.request_id || ""}`.toLowerCase().includes(needle);
  });
};

const timelineGroups = () => {
  const items = filteredTimelineItems();
  const groups = [];

  for (let index = 0; index < items.length; index += 1) {
    const item = items[index];
    const role = parseTimelineRole(item);
    const next = items[index + 1];
    if (role === "tool" && next && parseTimelineRole(next) === "result") {
      groups.push([item, next]);
      index += 1;
      continue;
    }
    groups.push([item]);
  }

  return groups;
};

const sessionCapabilities = () => state.sessionCapabilities || [];

const capabilityIcon = (type) => {
  const icons = {
    agent: "smart_toy",
    app: "apps",
    mcp: "hub",
    plugin: "extension",
    provider: "cloud",
    skill: "psychology",
    tool: "build",
    function: "functions"
  };
  return icons[type] || "deployed_code";
};

const renderCapabilityStrip = () => {
  const capabilities = sessionCapabilities();
  if (!capabilities.length) return "";

  const grouped = capabilities.reduce((groups, capability) => {
    const type = String(capability.capability_type || "capability");
    if (!groups.has(type)) groups.set(type, []);
    groups.get(type).push(capability);
    return groups;
  }, new Map());

  const groupsHtml = Array.from(grouped.entries())
    .map(([type, items]) => {
      const names = items
        .slice(0, 12)
        .map((item) => `
          <span class="inline-flex items-center gap-1 rounded-sm bg-surface-container-lowest border border-outline-variant/30 px-1.5 py-0.5 font-mono text-[10px] text-on-surface-variant" title="${escapeHtml(item.raw_ref?.path || "")}">
            ${escapeHtml(item.capability_name || "unknown")}
          </span>
        `)
        .join("");
      const overflow = items.length > 12 ? `<span class="font-mono text-[10px] text-on-surface-variant">+${escapeHtml(String(items.length - 12))}</span>` : "";
      return `
        <div class="flex items-start gap-2 min-w-0">
          <span class="material-symbols-outlined text-[15px] text-primary mt-0.5">${escapeHtml(capabilityIcon(type))}</span>
          <div class="min-w-0 flex-1">
            <div class="font-mono text-[10px] text-on-surface uppercase tracking-wide mb-1">${escapeHtml(type)} · ${escapeHtml(String(items.length))}</div>
            <div class="flex flex-wrap gap-1">${names}${overflow}</div>
          </div>
        </div>
      `;
    })
    .join("");

  return `
    <section class="mx-1 mb-3 rounded border border-outline-variant/40 bg-surface px-3 py-2">
      <div class="flex items-center gap-2 mb-2">
        <span class="material-symbols-outlined text-[16px] text-primary">deployed_code</span>
        <span class="font-headline text-xs font-bold text-on-surface">${escapeHtml(t("capabilities.title", "Visible capabilities"))}</span>
      </div>
      <div class="grid gap-3 md:grid-cols-2">${groupsHtml}</div>
    </section>
  `;
};

const renderDiagnosticsPanel = () => {
  const region = document.getElementById("diagnostics-panel-region");
  if (!region) return;

  if (!currentSessionSummary()) {
    region.innerHTML = `
      <div class="flex items-center gap-2 text-on-surface-variant">
        <span class="material-symbols-outlined text-[15px] text-primary">rule</span>
        <span class="font-headline text-xs font-bold uppercase tracking-wide">${escapeHtml(t("diagnostics.title", "Diagnostics"))}</span>
      </div>
      <div class="mt-2 text-[10px] text-on-surface-variant">${escapeHtml(t("state.no_session_selected", "选择一个会话"))}</div>
    `;
    return;
  }

  if (!state.sessionDiagnostics) {
    region.innerHTML = `
      <div class="flex items-center gap-2 text-on-surface-variant">
        <span class="material-symbols-outlined text-[15px] text-primary">rule</span>
        <span class="font-headline text-xs font-bold uppercase tracking-wide">${escapeHtml(t("diagnostics.title", "Diagnostics"))}</span>
      </div>
      <div class="mt-2 text-[10px] text-on-surface-variant">${escapeHtml(t("state.loading_detail", "加载详情..."))}</div>
    `;
    return;
  }

  const payload = state.sessionDiagnostics || {};
  const diagnostics = payload.diagnostics || {};
  const promptDiffs = payload.prompt_diffs || [];
  const skills = diagnostics.visible_skills || [];
  const capabilityInventory = diagnostics.capability_inventory || [];
  const skillStatus = diagnostics.skill_status || "partial";
  const statusClass =
    skillStatus === "available"
      ? "text-secondary bg-secondary/10 border-secondary/20"
      : skillStatus === "unavailable"
        ? "text-error bg-error/10 border-error/20"
        : "text-primary bg-primary/10 border-primary/20";
  const latestPromptDiff = promptDiffs[promptDiffs.length - 1];
  const addedLines = latestPromptDiff?.added_lines || [];
  const skillChips = skills.length
    ? skills
        .slice(0, 6)
        .map((skill) => `<span class="px-1.5 py-0.5 rounded-sm bg-surface-container-lowest text-[9px] text-on-surface-variant border border-outline-variant/30">${escapeHtml(skill)}</span>`)
        .join("")
    : `<span class="text-[10px] text-on-surface-variant">${escapeHtml(t("diagnostics.no_skills", "No skill facts"))}</span>`;
  const capabilityRows = capabilityInventory.length
    ? capabilityInventory
        .slice(0, 6)
        .map((group) => {
          const type = group.capability_type || "unknown";
          const names = group.capability_names || [];
          const count = group.count ?? names.length;
          const chips = names.length
            ? names
                .slice(0, 4)
                .map((name) => `<span class="px-1.5 py-0.5 rounded-sm bg-surface-container-lowest text-[9px] text-on-surface-variant border border-outline-variant/30">${escapeHtml(name)}</span>`)
                .join("")
            : `<span class="text-[10px] text-on-surface-variant">${escapeHtml(t("diagnostics.no_capabilities", "No capability facts"))}</span>`;
          const overflow = names.length > 4 ? `<span class="text-[9px] text-on-surface-variant">+${escapeHtml(String(names.length - 4))}</span>` : "";
          return `
            <div class="rounded-sm bg-surface-container-lowest border border-outline-variant/30 p-2">
              <div class="mb-1.5 flex items-center justify-between gap-2">
                <div class="flex min-w-0 items-center gap-1.5">
                  <span class="material-symbols-outlined text-[13px] text-primary">${capabilityIcon(type)}</span>
                  <span class="truncate font-mono text-[9px] uppercase text-on-surface">${escapeHtml(type)}</span>
                </div>
                <span class="font-mono text-[9px] text-on-surface-variant">${escapeHtml(String(count))}</span>
              </div>
              <div class="flex flex-wrap gap-1">${chips}${overflow}</div>
            </div>
          `;
        })
        .join("")
    : `<div class="text-[10px] text-on-surface-variant">${escapeHtml(t("diagnostics.no_capabilities", "No capability facts"))}</div>`;
  const promptPreview = addedLines.length
    ? addedLines
        .slice(0, 3)
        .map((line) => `<li class="truncate">${escapeHtml(line)}</li>`)
        .join("")
    : `<li class="text-on-surface-variant">${escapeHtml(t("diagnostics.no_prompt_diff", "No prompt diff"))}</li>`;

  region.innerHTML = `
    <div class="flex justify-between items-start gap-2">
      <div class="font-headline text-xs font-bold text-on-surface flex items-center gap-1.5 uppercase tracking-wide">
        <span class="material-symbols-outlined text-[14px] text-primary">rule</span>
        ${escapeHtml(t("diagnostics.title", "Diagnostics"))}
      </div>
      <span class="font-mono text-[9px] font-bold rounded-sm border px-1.5 py-0.5 ${statusClass}">${escapeHtml(skillStatus)}</span>
    </div>
    <div class="mt-3 grid grid-cols-3 gap-1">
      <div class="rounded-sm bg-surface-container-lowest border border-outline-variant/30 p-1.5">
        <div class="font-mono text-[13px] font-bold text-on-surface">${escapeHtml(String(diagnostics.prompt_diff_count || 0))}</div>
        <div class="text-[9px] text-on-surface-variant">prompt</div>
      </div>
      <div class="rounded-sm bg-surface-container-lowest border border-outline-variant/30 p-1.5">
        <div class="font-mono text-[13px] font-bold text-on-surface">${escapeHtml(String(diagnostics.tool_diff_count || 0))}</div>
        <div class="text-[9px] text-on-surface-variant">tool</div>
      </div>
      <div class="rounded-sm bg-surface-container-lowest border border-outline-variant/30 p-1.5">
        <div class="font-mono text-[13px] font-bold text-on-surface">${escapeHtml(String(diagnostics.skill_diff_count || 0))}</div>
        <div class="text-[9px] text-on-surface-variant">skill</div>
      </div>
    </div>
    <div class="mt-3">
      <div class="font-mono text-[9px] text-on-surface-variant uppercase mb-1">${escapeHtml(t("diagnostics.visible_skills", "Visible skills"))}</div>
      <div class="flex flex-wrap gap-1">${skillChips}</div>
    </div>
    <div class="mt-3">
      <div class="font-mono text-[9px] text-on-surface-variant uppercase mb-1">${escapeHtml(t("diagnostics.capability_inventory", "Capability inventory"))}</div>
      <div class="grid gap-1.5">${capabilityRows}</div>
    </div>
    <div class="mt-3 rounded-sm bg-surface-container-lowest border border-outline-variant/30 p-2">
      <div class="flex items-center gap-1.5 font-mono text-[9px] text-on-surface-variant uppercase mb-1">
        <span class="material-symbols-outlined text-[12px] text-primary">difference</span>
        ${escapeHtml(t("diagnostics.latest_prompt_delta", "Latest prompt delta"))}
      </div>
      <ul class="font-mono text-[10px] text-on-surface space-y-0.5">${promptPreview}</ul>
    </div>
  `;
};

const renderTranscript = () => {
  const region = document.getElementById("transcript-region");
  if (!region) return;
  const summary = currentSessionSummary();
  if (!summary) {
    region.innerHTML = `
      <div class="h-full flex items-center justify-center text-center text-on-surface-variant">
        <div>
          <div class="w-14 h-14 mx-auto mb-4 rounded bg-surface-container-high border border-outline-variant/20 flex items-center justify-center">
            <span class="material-symbols-outlined text-[28px] text-primary">forum</span>
          </div>
          <h2 class="text-on-surface font-semibold mb-2">${escapeHtml(t("state.no_session_selected", "选择一个会话"))}</h2>
          <p class="text-xs">${escapeHtml(t("state.session_hint", "从左侧选择 Codex / opencode 会话查看完整上下文。"))}</p>
        </div>
      </div>
    `;
    return;
  }

  if (!state.sessionDetail) {
    region.innerHTML = `<div class="p-6 text-xs text-on-surface-variant">${escapeHtml(t("state.loading_timeline", "加载时间线..."))}</div>`;
    return;
  }

  const groups = timelineGroups();
  const capabilityStrip = renderCapabilityStrip();
  if (!groups.length) {
    region.innerHTML = `${capabilityStrip}<div class="p-6 text-xs text-on-surface-variant">${escapeHtml(t("state.no_timeline_items", "没有匹配的时间线条目。"))}</div>${renderTimelinePagination()}`;
    return;
  }

  region.innerHTML = `${capabilityStrip}${groups.map(renderTimelineGroup).join("")}${renderTimelinePagination()}`;
};

const renderTimelineGroup = (items) => {
  const primary = items[0];
  if (items.length === 1) return renderTimelineItem(primary, items);
  return renderTimelineItem(primary, items);
};

const renderTimelinePagination = () => {
  const hasMore =
    state.sessionEventsNextCursor !== null && state.sessionEventsNextCursor !== undefined;
  if (!hasMore) return "";
  return `
    <div class="px-10 py-2">
      <button class="w-full px-2.5 py-1.5 rounded-sm bg-surface-container-high text-on-surface text-[11px] font-medium hover:bg-outline-variant transition-colors border border-outline-variant" data-load-more-session-events type="button">
        ${escapeHtml(state.sessionEventsLoadingMore ? t("state.loading_timeline", "加载时间线...") : t("action.load_more", "加载更多"))}
      </button>
    </div>
  `;
};

const groupSelected = (items) => items.every((item) => state.selectedItemIds.has(item.request_id));

const renderTimelineItem = (item, groupItems = [item]) => {
  const role = parseTimelineRole(item);
  const selected = groupSelected(groupItems);
  const details = groupItems.map((groupItem) => state.detailCache.get(groupItem.request_id)).filter(Boolean);
  const detailLoaded = details.length > 0;
  const text = cleanSummary(item);
  const secondary = groupItems[1];
  const isTechnical = ["instruction", "context", "tool", "result", "capability"].includes(role);
  const groupIds = groupItems.map((groupItem) => groupItem.request_id).join(",");
  const groupSummary = secondary ? `${text}\n${cleanSummary(secondary)}` : text;
  return `
    <article class="flex group ${
      selected ? "bg-primary/5 border border-primary/20" : "hover:bg-surface"
    } transition-colors rounded p-1 mb-2" data-timeline-item="${escapeHtml(item.request_id)}" data-timeline-group="${escapeHtml(groupIds)}">
      <div class="w-8 shrink-0 flex flex-col items-center pt-2">
        <input class="w-3.5 h-3.5 rounded-sm ${selected ? "bg-primary border-primary text-on-primary" : "bg-surface border-outline text-primary"} focus:ring-primary cursor-pointer" data-select-item="${escapeHtml(groupIds)}" ${selected ? "checked" : ""} type="checkbox" />
      </div>
      <div class="w-10 shrink-0 pt-1.5 flex justify-center">
        <div class="w-6 h-6 rounded ${timelineIconShellClass(role)} flex items-center justify-center">
          <span class="material-symbols-outlined text-[14px]" ${role === "assistant" ? 'style="font-variation-settings: \'FILL\' 1;"' : ""}>${escapeHtml(roleIcon(role))}</span>
        </div>
      </div>
      <div class="flex-1 min-w-0 pt-1.5 pb-2 pr-4">
        <div class="flex items-baseline gap-2 mb-1">
          <span class="font-headline text-xs font-bold ${roleColorClass(role)} uppercase tracking-wider">${escapeHtml(roleCopy(role))}</span>
          <span class="font-mono text-[9px] text-on-surface-variant/60">${escapeHtml(formatTime(item.started_at_ms))}</span>
          ${role === "tool" ? `<span class="ml-auto text-[10px] text-secondary font-mono font-bold bg-secondary/10 px-1.5 py-0.5 rounded border border-secondary/20 flex items-center gap-1"><span class="w-1.5 h-1.5 rounded-full bg-secondary"></span>${escapeHtml(detailLoaded ? t("action.expand", "Loaded") : "Running")}</span>` : ""}
        </div>
        ${isTechnical ? renderTechnicalSummary(item, groupSummary, detailLoaded, groupIds, secondary) : `<div class="text-[13px] text-on-surface leading-relaxed whitespace-pre-wrap">${escapeHtml(text)}</div>`}
        ${detailLoaded ? details.map(renderDetailBlock).join("") : ""}
      </div>
    </article>
  `;
};

const renderTechnicalSummary = (item, text, detailLoaded, groupIds, secondary) => `
  <button class="bg-surface-container-lowest rounded border border-surface-variant/40 cursor-pointer hover:border-primary/40 transition-colors w-full text-left" data-detail-toggle="${escapeHtml(groupIds)}" type="button">
    <span class="px-3 py-2 flex items-center justify-between">
      <span class="flex items-center gap-2 min-w-0">
        <span class="material-symbols-outlined text-[16px] text-primary">${detailLoaded ? "expand_more" : "chevron_right"}</span>
        <span class="font-mono text-xs text-on-surface truncate">${escapeHtml(text)}</span>
        ${secondary ? `<span class="text-[10px] text-secondary font-mono bg-secondary/10 px-1.5 py-0.5 rounded border border-secondary/20">${escapeHtml(t("timeline.call_and_result", "调用+结果"))}</span>` : ""}
      </span>
      <span class="text-[10px] text-on-surface-variant">${escapeHtml(detailLoaded ? t("action.collapse", "收起") : t("action.expand", "展开"))}</span>
    </span>
  </button>
`;

const renderDetailBlock = (payload) => {
  const request = requestDetailFromPayload(payload);
  const rollout = request.codex_rollout || {};
  const blocks = [];
  if (rollout.agents_instructions) blocks.push(renderPreBlock(t("inspector.agents_instructions", "AGENTS / 工作方式指令"), rollout.agents_instructions));
  if (rollout.ide_context) blocks.push(renderPreBlock(t("inspector.ide_context", "IDE / 浏览器上下文"), rollout.ide_context));
  if (rollout.user_request) blocks.push(renderPreBlock(t("inspector.user_request", "用户请求"), rollout.user_request));
  if (rollout.full_text) blocks.push(renderPreBlock(t("inspector.full_context", "完整上下文原文"), rollout.full_text));
  if (rollout.tool_name) blocks.push(renderKeyValueBlock(t("inspector.tool_name", "工具名称"), rollout.tool_name));
  if (rollout.tool_arguments) blocks.push(renderPreBlock(t("inspector.tool_arguments", "工具参数"), rollout.tool_arguments));
  if (rollout.tool_output) blocks.push(renderPreBlock(t("inspector.tool_output", "工具返回"), rollout.tool_output));
  if (request.body_text && !blocks.length) blocks.push(renderPreBlock(t("inspector.raw_payload", "原始载荷"), request.body_text));
  if (!blocks.length) blocks.push(renderPreBlock(t("inspector.raw_payload", "原始载荷"), JSON.stringify(request, null, 2)));

  return `
    <div class="mt-2 bg-surface-container-lowest rounded border border-outline-variant/20 overflow-hidden">
      ${blocks.join("")}
    </div>
  `;
};

const requestDetailFromPayload = (payload) => {
  if (payload?.request) return payload.request;
  const event = payload?.event;
  if (!event) return {};
  if (event.status) {
    return {
      request_id: event.event_id,
      body_text: event.detail || JSON.stringify(event, null, 2)
    };
  }

  const sourceKind = String(event.source?.kind || "");
  const detail = event.detail || {};
  const raw = event.raw_json ?? detail;
  return {
    request_id: event.event_id,
    provider: event.source?.channel || event.source?.kind,
    model: event.event_kind,
    request_summary: event.summary,
    artifact_path: event.artifact?.path,
    body_text: JSON.stringify(raw, null, 2),
    codex_rollout: sourceKind === "codex_rollout" ? detail : null,
    observer_event: sourceKind === "observer_event" ? detail : null
  };
};

const renderKeyValueBlock = (label, value) => `
  <div class="px-3 py-2 border-b border-outline-variant/10">
    <span class="font-mono text-[10px] text-on-surface-variant">${escapeHtml(label)}</span>
    <span class="ml-2 font-mono text-xs text-on-surface">${escapeHtml(value)}</span>
  </div>
`;

const renderPreBlock = (label, value) => `
  <details class="border-b border-outline-variant/10" open>
    <summary class="px-3 py-2 cursor-pointer font-mono text-[10px] text-on-surface-variant hover:text-on-surface">${escapeHtml(label)}</summary>
    <pre class="p-3 pt-1 overflow-x-auto whitespace-pre-wrap font-mono text-[11px] text-on-surface-variant leading-relaxed">${escapeHtml(value)}</pre>
  </details>
`;

const renderSelectionToolbar = () => {
  const toolbar = document.getElementById("selection-toolbar-region");
  const countRegion = document.getElementById("selection-count-region");
  if (!toolbar || !countRegion) return;
  const count = state.selectedItemIds.size;
  toolbar.classList.toggle("hidden", count === 0);
  toolbar.classList.toggle("flex", count > 0);
  countRegion.textContent =
    state.language === "zh-CN" ? `已选 ${count} 条` : `${count} selected`;
  renderAnalysisContext();
};

const selectedItems = () => {
  const items = state.sessionDetail?.session?.timeline_items || [];
  return items.filter((item) => state.selectedItemIds.has(item.request_id));
};

const renderAnalysisContext = () => {
  const summaryRegion = document.getElementById("analysis-context-summary");
  const chipsRegion = document.getElementById("analysis-context-chips");
  if (!summaryRegion || !chipsRegion) return;
  const items = selectedItems();
  summaryRegion.textContent =
    state.language === "zh-CN" ? `${items.length} 条时间线内容已附加` : `${items.length} transcript items attached`;
  chipsRegion.innerHTML = items
    .slice(0, 8)
    .map((item) => {
      const role = parseTimelineRole(item);
      return `<span class="px-1.5 py-0.5 rounded bg-surface-container-lowest text-[9px] text-on-surface-variant border border-outline-variant/20 truncate max-w-[130px]">${escapeHtml(
        roleCopy(role)
      )}: ${escapeHtml(cleanSummary(item))}</span>`;
    })
    .join("");
};

const loadSessions = async () => {
  const payload = await apiJson(`/api/sessions?limit=${SESSION_PAGE_LIMIT}`);
  state.sessions = payload.sessions || payload.items || [];
  state.sessionNextCursor = payload.next_cursor ?? null;
  const visibleSessions = filteredSessions();
  if (
    state.selectedSessionId &&
    !visibleSessions.some((session) => session.session_id === state.selectedSessionId)
  ) {
    state.selectedSessionId = null;
  }
  if (!state.selectedSessionId && visibleSessions.length) {
    const initial = document.body.dataset.initialSessionId;
    state.selectedSessionId = visibleSessions.some((session) => session.session_id === initial)
      ? initial
      : visibleSessions[0].session_id;
  }
  renderRuntimeList();
  renderSessionList();
  renderSessionHeader();
  renderDiagnosticsPanel();
  if (state.selectedSessionId) await loadSessionDetail(state.selectedSessionId);
};

const loadMoreSessions = async () => {
  if (state.sessionLoadingMore) return;
  if (state.sessionNextCursor === null || state.sessionNextCursor === undefined) return;
  state.sessionLoadingMore = true;
  renderSessionList();
  try {
    const payload = await apiJson(
      `/api/sessions?limit=${SESSION_PAGE_LIMIT}&cursor=${state.sessionNextCursor}`
    );
    appendUniqueById(state.sessions, payload.sessions || payload.items || [], "session_id");
    state.sessionNextCursor = payload.next_cursor ?? null;
    renderRuntimeList();
    renderSessionList();
  } finally {
    state.sessionLoadingMore = false;
    renderSessionList();
  }
};

const loadSessionDetail = async (sessionId) => {
  if (!sessionId) return;
  state.selectedSessionId = sessionId;
  state.selectedItemIds.clear();
  state.sessionDetail = null;
  state.sessionCapabilities = [];
  state.sessionDiagnostics = null;
  state.sessionEventsNextCursor = null;
  state.sessionEventsLoadingMore = false;
  renderSessionList();
  renderSessionHeader();
  renderTranscript();
  renderDiagnosticsPanel();
  renderSelectionToolbar();
  const [payload, capabilityPayload, diagnosticsPayload, eventsPayload] = await Promise.all([
    apiJson(`/api/sessions/${sessionId}`),
    apiJson(`/api/sessions/${sessionId}/capabilities`).catch(() => ({ capabilities: [] })),
    apiJson(`/api/sessions/${sessionId}/diagnostics`).catch(() => ({
      diagnostics: {},
      prompt_diffs: [],
      tool_visibility_diffs: [],
      skill_visibility_diffs: [],
      skill_diagnostics: []
    })),
    apiJson(`/api/sessions/${sessionId}/events?limit=${SESSION_EVENT_PAGE_LIMIT}`).catch(() => ({
      timeline_items: []
    }))
  ]);
  if (payload.session) {
    payload.session.timeline_items = eventsPayload.timeline_items || eventsPayload.items || [];
  }
  state.sessionDetail = payload;
  state.sessionCapabilities = capabilityPayload.capabilities || capabilityPayload.items || [];
  state.sessionDiagnostics = diagnosticsPayload;
  state.sessionEventsNextCursor = eventsPayload.next_cursor ?? null;
  renderSessionHeader();
  renderTranscript();
  renderDiagnosticsPanel();
};

const loadMoreSessionEvents = async () => {
  const sessionId = state.selectedSessionId;
  if (!sessionId || !state.sessionDetail?.session) return;
  if (state.sessionEventsLoadingMore) return;
  if (state.sessionEventsNextCursor === null || state.sessionEventsNextCursor === undefined) return;
  state.sessionEventsLoadingMore = true;
  renderTranscript();
  try {
    const payload = await apiJson(
      `/api/sessions/${sessionId}/events?limit=${SESSION_EVENT_PAGE_LIMIT}&cursor=${state.sessionEventsNextCursor}`
    );
    const existing = state.sessionDetail.session.timeline_items || [];
    state.sessionDetail.session.timeline_items = appendUniqueById(
      existing,
      payload.timeline_items || payload.items || [],
      "request_id"
    );
    state.sessionEventsNextCursor = payload.next_cursor ?? null;
  } finally {
    state.sessionEventsLoadingMore = false;
    renderTranscript();
  }
};

const loadRequestDetail = async (requestId) => {
  if (!requestId) return;
  if (state.detailCache.has(requestId)) {
    state.detailCache.delete(requestId);
    renderTranscript();
    return;
  }
  state.detailCache.set(requestId, { request: { body_text: t("state.loading_detail", "加载详情...") } });
  renderTranscript();
  try {
    const payload = await apiJson(detailApiPathForRequestId(requestId));
    state.detailCache.set(requestId, payload);
  } catch (error) {
    state.detailCache.set(requestId, { request: { body_text: `${t("panel.load_failed", "加载失败")}: ${error.message}` } });
  }
  renderTranscript();
};

const loadRequestDetails = async (requestIds) => {
  const ids = requestIds.filter(Boolean);
  if (!ids.length) return;
  const allLoaded = ids.every((id) => state.detailCache.has(id));
  if (allLoaded) {
    ids.forEach((id) => state.detailCache.delete(id));
    renderTranscript();
    return;
  }

  ids.forEach((id) => {
    if (!state.detailCache.has(id)) {
      state.detailCache.set(id, { request: { body_text: t("state.loading_detail", "加载详情...") } });
    }
  });
  renderTranscript();

  await Promise.all(
    ids
      .filter((id) => state.detailCache.get(id)?.request?.body_text === t("state.loading_detail", "加载详情..."))
      .map(async (id) => {
        try {
          state.detailCache.set(id, await apiJson(detailApiPathForRequestId(id)));
        } catch (error) {
          state.detailCache.set(id, { request: { body_text: `${t("panel.load_failed", "加载失败")}: ${error.message}` } });
        }
      })
  );
  renderTranscript();
};

const refreshAll = async () => {
  try {
    await loadSessions();
    setWsStatus("live");
  } catch (error) {
    setWsStatus("offline");
    console.warn(`${t("panel.load_failed", "加载失败")}: ${error.message}`);
  }
};

const setWsStatus = (status) => {
  const region = document.getElementById("ws-status-region");
  if (!region) return;
  const live = status === "live";
  region.classList.toggle("text-secondary", live);
  region.classList.toggle("text-error", !live);
  region.innerHTML = `
    <span class="w-1.5 h-1.5 rounded-full ${live ? "bg-primary" : "bg-error"}"></span>
    <span>${escapeHtml(live ? t("ws.live", "Live") : t("ws.offline", "Offline"))}</span>
  `;
};

const toggleSidebar = (side) => {
  const key = side === "left" ? "leftCollapsed" : "rightCollapsed";
  const id = side === "left" ? "left-sidebar" : "right-sidebar";
  const node = document.getElementById(id);
  if (!node) return;
  state[key] = !state[key];
  node.classList.toggle("hidden", state[key]);
};

const addSelectionToAnalysis = () => {
  const region = document.getElementById("analysis-message-region");
  if (!region) return;
  const items = selectedItems();
  const title = currentSessionSummary() ? sessionTitle(currentSessionSummary()) : "";
  region.insertAdjacentHTML(
    "beforeend",
    `
      <div class="flex flex-col items-end">
        <div class="bg-surface-bright text-on-surface p-2.5 rounded-lg rounded-tr-sm max-w-[90%] text-[13px] leading-relaxed border border-outline-variant/10 shadow-sm">
          ${escapeHtml(state.language === "zh-CN" ? `把 ${items.length} 条内容加入分析：${title}` : `Attach ${items.length} items for analysis: ${title}`)}
        </div>
        <div class="text-[9px] text-on-surface-variant mt-1 font-mono">${escapeHtml(formatTime(Date.now()))}</div>
      </div>
    `
  );
  region.scrollTop = region.scrollHeight;
};

const bindEvents = () => {
  document.addEventListener("click", (event) => {
    const sessionButton = event.target.closest("[data-session-id]");
    if (sessionButton) {
      void loadSessionDetail(sessionButton.getAttribute("data-session-id"));
      return;
    }

    if (event.target.closest("[data-load-more-sessions]")) {
      void loadMoreSessions();
      return;
    }

    if (event.target.closest("[data-load-more-session-events]")) {
      void loadMoreSessionEvents();
      return;
    }

    const runtimeButton = event.target.closest("[data-runtime-value]");
    if (runtimeButton) {
      state.selectedRuntime = runtimeButton.getAttribute("data-runtime-value") || "all";
      renderRuntimeList();
      renderSessionList();
      return;
    }

    const detailButton = event.target.closest("[data-detail-toggle]");
    if (detailButton) {
      const requestIds = String(detailButton.getAttribute("data-detail-toggle") || "")
        .split(",")
        .filter(Boolean);
      void loadRequestDetails(requestIds);
      return;
    }

    if (event.target.closest("[data-selected-only]")) {
      state.selectedOnly = !state.selectedOnly;
      renderTranscript();
      return;
    }

    if (event.target.closest("[data-clear-selection]")) {
      state.selectedItemIds.clear();
      renderTranscript();
      renderSelectionToolbar();
      return;
    }

    if (event.target.closest("[data-add-to-analysis]")) {
      addSelectionToAnalysis();
      return;
    }

    const layoutButton = event.target.closest("[data-layout-toggle]");
    if (layoutButton) {
      toggleSidebar(layoutButton.getAttribute("data-layout-toggle"));
      return;
    }

    if (event.target.closest("[data-theme-cycle]")) {
      const next = state.theme === "dark" ? "light" : "dark";
      window.localStorage.setItem("prismtrace-console-theme", next);
      applyTheme(next);
      return;
    }

    if (event.target.closest("[data-language-cycle]")) {
      const next = state.language === "zh-CN" ? "en-US" : "zh-CN";
      window.localStorage.setItem("prismtrace-console-language", next);
      void initLanguage(next);
      return;
    }

    if (event.target.closest("[data-refresh]")) {
      void refreshAll();
      return;
    }

    if (event.target.closest("[data-scroll-bottom]")) {
      const transcript = document.getElementById("transcript-region");
      if (transcript) transcript.scrollTop = transcript.scrollHeight;
    }
  });

  document.addEventListener("change", (event) => {
    const checkbox = event.target.closest("[data-select-item]");
    if (!checkbox) return;
    const requestIds = String(checkbox.getAttribute("data-select-item") || "")
      .split(",")
      .filter(Boolean);
    if (checkbox.checked) {
      requestIds.forEach((requestId) => state.selectedItemIds.add(requestId));
    } else {
      requestIds.forEach((requestId) => state.selectedItemIds.delete(requestId));
    }
    renderTranscript();
    renderSelectionToolbar();
  });

  const globalSearch = document.getElementById("global-search");
  if (globalSearch) {
    globalSearch.addEventListener("input", () => {
      state.globalSearch = globalSearch.value;
      renderSessionList();
    });
  }

  const runtimeSearch = document.getElementById("runtime-search");
  if (runtimeSearch) {
    runtimeSearch.addEventListener("input", () => {
      state.runtimeSearch = runtimeSearch.value;
      renderRuntimeList();
      renderSessionList();
    });
  }

  const timelineKeyword = document.getElementById("timeline-keyword");
  if (timelineKeyword) {
    timelineKeyword.addEventListener("input", () => {
      state.timelineKeyword = timelineKeyword.value;
      renderTranscript();
    });
  }
};

const initLanguage = async (language) => {
  state.language = normalizeLanguage(language);
  state.translations = await loadLanguageResource(state.language);
  applyStaticTranslations();
  if (state.sessions.length || state.sessionDetail) {
    renderRuntimeList();
    renderSessionList();
    renderSessionHeader();
    renderTranscript();
    renderDiagnosticsPanel();
    renderSelectionToolbar();
  }
};

const init = async () => {
  applyTheme(readThemePreference());
  observeLocalIcons();
  bindEvents();
  await initLanguage(readLanguagePreference());
  await refreshAll();
};

void init();
