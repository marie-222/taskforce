const configNode = document.getElementById("taskforce-index-config");
const config = configNode ? JSON.parse(configNode.textContent ?? "{}") : {};
const labels = config.labels ?? {};
const statusLabels = config.status_labels ?? {};
const configuredStatusOrder = Array.isArray(config.status_order)
  ? config.status_order
  : null;
const openStatuses = new Set(
  Array.isArray(config.open_statuses) ? config.open_statuses : ["active"]
);

const taskList = document.getElementById("task-list");
const emptyState = document.getElementById("empty");
const refreshButton = document.getElementById("refresh");
const quickFilterInput = document.getElementById("quick-filter");
const searchForm = document.getElementById("search-form");
const searchTextarea = document.getElementById("search-where");
const searchQInput = document.getElementById("search-q");
const searchTagInput = document.getElementById("search-tag");
const searchTagSuggestions = document.getElementById("search-tag-suggestions");
const searchStatusInputs = Array.from(
  document.querySelectorAll('input[name="status"]')
);
const statusOrder =
  configuredStatusOrder ?? [
    "active",
    "unstarted",
    "waiting",
    "suspended",
    "done",
    "abandoned",
    "mistaken",
    "duplicated",
  ];
const apiUrl = config.api_url ?? "/api/tasks";
const searchApiBase = config.search_api_base ?? null;
const tagSuggestApi = config.tag_suggest_api ?? null;
const showQuickFilter = config.show_quick_filter ?? true;
let knownTags = new Set();
let tagSuggestionItems = [];
let activeTagSuggestionIndex = -1;

function initializeNavDrawer() {
  const toggle = document.getElementById("nav-toggle");
  const close = document.getElementById("nav-close");
  const drawer = document.getElementById("nav-drawer");
  const backdrop = document.getElementById("nav-backdrop");
  if (!toggle || !drawer || !backdrop) {
    return;
  }

  function setOpen(nextOpen) {
    toggle.setAttribute("aria-expanded", String(nextOpen));
    drawer.hidden = !nextOpen;
    backdrop.hidden = !nextOpen;
    document.body.style.overflow = nextOpen ? "hidden" : "";
  }

  toggle.addEventListener("click", () => {
    setOpen(toggle.getAttribute("aria-expanded") !== "true");
  });
  close?.addEventListener("click", () => setOpen(false));
  backdrop.addEventListener("click", () => setOpen(false));
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      setOpen(false);
    }
  });
}

function label(name, fallback) {
  return labels[name] ?? fallback;
}

function statusLabel(status) {
  return statusLabels[status] ?? status ?? "unknown";
}

function formatDate(value) {
  return value ?? null;
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll("\"", "&quot;")
    .replaceAll("'", "&#39;");
}

function createTaskItem(task) {
  const item = document.createElement("li");
  const deadline = formatDate(task.deadline);
  const target = formatDate(task.target_date);
  const launch = formatDate(task.launch_date);
  const descriptionPreview = task.description_preview;
  const hashtags = Array.isArray(task.tags)
    ? task.tags
        .map(
          (tag) =>
            `<a class="task-hashtag" href="/tags/${encodeURIComponent(tag)}">#${escapeHtml(tag)}</a>`,
        )
        .join("")
    : "";
  item.innerHTML = `
    <span class="task-id">#${task.id ?? "?"}</span>
    <div class="task-main">
      <div class="task-title-row">
        <a class="task-link" href="/tasks/${task.id ?? ""}"></a>
        <span class="task-status task-status--${task.status}">${statusLabel(task.status)}</span>
      </div>
      ${descriptionPreview ? `<div class="task-description-preview">${escapeHtml(descriptionPreview)}</div>` : ""}
      ${hashtags ? `<div class="task-hashtags">${hashtags}</div>` : ""}
      <div class="task-meta">
        <span class="task-meta-item task-meta-item--deadline">${label("deadline", "Deadline")} ${deadline ?? label("no_deadline", "No deadline")}</span>
        ${target ? `<span class="task-meta-item">${label("target", "Target")} ${target}</span>` : ""}
        ${launch ? `<span class="task-meta-item">${label("launch", "Launch")} ${launch}</span>` : ""}
      </div>
    </div>
    <span class="task-urgency">${label("urgency", "urgency")} ${Number(task.urgency ?? 0).toFixed(1)}</span>
  `;
  item.querySelector(".task-link").textContent = task.title;
  return item;
}

function groupTasks(tasks) {
  const groups = new Map();
  for (const status of statusOrder) {
    groups.set(status, []);
  }

  for (const task of tasks) {
    const status = task.status;
    if (!groups.has(status)) {
      groups.set(status, []);
    }
    groups.get(status).push(task);
  }

  return groups;
}

function currentQuickFilter() {
  return String(quickFilterInput?.value ?? "").trim();
}

function splitWhereLines(value) {
  return String(value ?? "")
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

function currentRawSearchClauses() {
  if (!searchTextarea) {
    return [];
  }
  return splitWhereLines(searchTextarea.value);
}

function currentStructuredSearchClauses() {
  if (!searchForm) {
    return [];
  }

  const clauses = [];
  const selectedStatuses = searchStatusInputs
    .filter((input) => input.checked)
    .map((input) => input.value);
  if (selectedStatuses.length === 1) {
    clauses.push(`status = '${selectedStatuses[0]}'`);
  } else if (selectedStatuses.length > 1) {
    clauses.push(
      `status in (${selectedStatuses.map((status) => `'${status}'`).join(", ")})`
    );
  }

  const { confirmedTags } = classifyTagInput();
  for (const tag of confirmedTags) {
    clauses.push(`tag = '${tag.replaceAll("'", "''")}'`);
  }

  return clauses;
}

function currentSearchClauses() {
  return [...currentStructuredSearchClauses(), ...currentRawSearchClauses()];
}

function currentSearchText() {
  if (!searchForm) {
    return currentQuickFilter();
  }
  return String(searchQInput?.value ?? "").trim();
}

function syncSearchUrl(clauses) {
  const url = new URL(window.location.href);
  const query = currentSearchText();
  url.search = "";
  if (searchForm) {
    const selectedStatuses = searchStatusInputs
      .filter((input) => input.checked)
      .map((input) => input.value);
    for (const status of selectedStatuses) {
      url.searchParams.append("status", status);
    }
    const tag = String(searchTagInput?.value ?? "").trim();
    if (tag) {
      url.searchParams.set("tag", tag);
    }
    const rawWhere = currentRawSearchClauses();
    if (rawWhere.length > 0) {
      url.searchParams.set("where", rawWhere.join("\n"));
    }
  } else if (clauses.length > 0) {
    url.searchParams.set("where", clauses.join("\n"));
  }
  if (query) {
    url.searchParams.set("q", query);
  }
  window.history.replaceState({}, "", url);
}

function effectiveApiUrl() {
  if (!searchForm) {
    return apiUrl;
  }

  const clauses = currentSearchClauses();
  const query = currentSearchText();
  if (clauses.length === 0 && !query) {
    return null;
  }

  const url = new URL(searchApiBase ?? apiUrl, window.location.origin);
  if (clauses.length > 0) {
    url.searchParams.set("where", clauses.join("\n"));
  }
  if (query) {
    url.searchParams.set("q", query);
  }
  return url.toString();
}

function initializeSearchForm() {
  const params = new URLSearchParams(window.location.search);
  const initialQuickFilter = params.get("q");
  if (initialQuickFilter) {
    if (searchForm && searchQInput) {
      searchQInput.value = initialQuickFilter;
    } else if (quickFilterInput) {
      quickFilterInput.value = initialQuickFilter;
    }
  }

  if (!searchForm) {
    return;
  }

  const initialStatuses = new Set(params.getAll("status"));
  for (const input of searchStatusInputs) {
    input.checked = initialStatuses.has(input.value);
  }

  const initialTag = params.get("tag");
  if (initialTag && searchTagInput) {
    searchTagInput.value = initialTag;
  }

  const initialWhere = params.get("where");
  if (initialWhere && searchTextarea) {
    searchTextarea.value = initialWhere;
  }

  searchForm.addEventListener("submit", (event) => {
    event.preventDefault();
    const clauses = currentSearchClauses();
    syncSearchUrl(clauses);
    loadTasks().catch(console.error);
  });
}

let tagSuggestTimer = null;

function scheduleTagSuggestionsUpdate(delay = 0) {
  if (tagSuggestTimer) {
    window.clearTimeout(tagSuggestTimer);
  }
  tagSuggestTimer = window.setTimeout(() => {
    updateTagSuggestions().catch(console.error);
  }, delay);
}

function hideTagSuggestions() {
  if (!searchTagSuggestions) {
    return;
  }
  searchTagSuggestions.hidden = true;
  searchTagSuggestions.innerHTML = "";
  tagSuggestionItems = [];
  activeTagSuggestionIndex = -1;
}

function tokenizeTagInput(rawValue) {
  return String(rawValue ?? "")
    .split(/\s+/)
    .filter((token) => token.length > 0);
}

function applyTagSuggestion(tag) {
  if (!searchTagInput) {
    return;
  }

  const rawValue = String(searchTagInput.value ?? "");
  const endsWithWhitespace = /\s$/.test(rawValue);
  const tokens = tokenizeTagInput(rawValue);
  const lastToken = tokens.at(-1) ?? null;
  const hasExactLastToken = lastToken != null && knownTags.has(lastToken);

  if (tokens.length === 0 || endsWithWhitespace || hasExactLastToken) {
    tokens.push(tag);
  } else {
    tokens[tokens.length - 1] = tag;
  }

  searchTagInput.value = `${tokens.join(" ")} `;
  searchTagInput.focus();
  scheduleTagSuggestionsUpdate();
}

function setActiveTagSuggestion(nextIndex) {
  activeTagSuggestionIndex = nextIndex;
  tagSuggestionItems.forEach((item, index) => {
    item.classList.toggle("search-tag-suggestion--active", index === nextIndex);
  });
}

function moveActiveTagSuggestion(delta) {
  if (tagSuggestionItems.length === 0) {
    return;
  }

  if (activeTagSuggestionIndex === -1) {
    setActiveTagSuggestion(delta > 0 ? 0 : tagSuggestionItems.length - 1);
    return;
  }

  const nextIndex =
    (activeTagSuggestionIndex + delta + tagSuggestionItems.length) %
    tagSuggestionItems.length;
  setActiveTagSuggestion(nextIndex);
}

function acceptActiveTagSuggestion() {
  if (
    activeTagSuggestionIndex < 0 ||
    activeTagSuggestionIndex >= tagSuggestionItems.length
  ) {
    return false;
  }

  const tag = tagSuggestionItems[activeTagSuggestionIndex].dataset.tag;
  if (!tag) {
    return false;
  }
  applyTagSuggestion(tag);
  return true;
}

async function updateTagSuggestions() {
  if (!searchTagInput || !searchTagSuggestions || !tagSuggestApi) {
    return;
  }

  const { confirmedTags, partialTag } = classifyTagInput();
  const url = new URL(tagSuggestApi, window.location.origin);
  if (partialTag) {
    url.searchParams.set("q", partialTag);
  } else {
    url.searchParams.set("all", "true");
  }

  const response = await fetch(url.toString());
  const tags = await response.json();
  const availableTags = tags.filter(
    (candidate) => !confirmedTags.includes(candidate)
  );
  if (availableTags.length === 0) {
    hideTagSuggestions();
    return;
  }

  searchTagSuggestions.innerHTML = "";
  tagSuggestionItems = [];
  for (const tag of availableTags) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "search-tag-suggestion";
    button.textContent = `#${tag}`;
    button.dataset.tag = tag;
    button.addEventListener("mousedown", (event) => {
      event.preventDefault();
    });
    button.addEventListener("click", () => {
      applyTagSuggestion(tag);
    });
    searchTagSuggestions.appendChild(button);
    tagSuggestionItems.push(button);
  }
  searchTagSuggestions.hidden = false;
  setActiveTagSuggestion(-1);
}

function classifyTagInput() {
  const rawValue = String(searchTagInput?.value ?? "");
  const endsWithWhitespace = /\s$/.test(rawValue);
  const tokens = rawValue.split(/\s+/).filter((token) => token.length > 0);
  if (tokens.length === 0) {
    return { confirmedTags: [], partialTag: null };
  }

  const lastToken = tokens.at(-1) ?? null;
  const hasExactLastToken = lastToken != null && knownTags.has(lastToken);
  const candidateConfirmed =
    endsWithWhitespace || hasExactLastToken ? tokens : tokens.slice(0, -1);
  const confirmedTags = candidateConfirmed.filter((token) => knownTags.has(token));
  const partialTag =
    endsWithWhitespace || hasExactLastToken ? null : lastToken;
  return { confirmedTags, partialTag };
}

function initializeTagSuggestions() {
  if (!searchTagInput || !searchTagSuggestions || !tagSuggestApi) {
    return;
  }

  searchTagInput.addEventListener("input", () => {
    scheduleTagSuggestionsUpdate(120);
  });

  searchTagInput.addEventListener("focus", () => {
    updateTagSuggestions().catch(console.error);
  });

  searchTagInput.addEventListener("click", () => {
    updateTagSuggestions().catch(console.error);
  });

  searchTagInput.addEventListener("keydown", (event) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      moveActiveTagSuggestion(1);
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      moveActiveTagSuggestion(-1);
      return;
    }
    if (event.key === "Enter" || event.key === "Tab") {
      if (!searchTagSuggestions.hidden && acceptActiveTagSuggestion()) {
        event.preventDefault();
        return;
      }
    }
    if (event.key === "Escape") {
      event.preventDefault();
      hideTagSuggestions();
    }
  });

  document.addEventListener("click", (event) => {
    if (
      searchTagInput.contains(event.target) ||
      searchTagSuggestions.contains(event.target)
    ) {
      return;
    }
    hideTagSuggestions();
  });
}

async function initializeKnownTags() {
  if (!tagSuggestApi) {
    return;
  }

  const url = new URL(tagSuggestApi, window.location.origin);
  url.searchParams.set("all", "true");
  const response = await fetch(url.toString());
  const tags = await response.json();
  knownTags = new Set(tags);
}

async function loadTasks() {
  taskList.innerHTML = "";
  const requestUrl = effectiveApiUrl();
  if (!requestUrl) {
    emptyState.hidden = false;
    emptyState.textContent = label(
      "search_prompt_builder",
      "Enter a search term or at least one filter."
    );
    return;
  }

  const response = await fetch(requestUrl);
  const tasks = await response.json();
  const filterActive =
    currentSearchText().length > 0 || currentSearchClauses().length > 0;
  emptyState.hidden = tasks.length !== 0;
  emptyState.textContent = filterActive
    ? label("no_filtered_tasks", "No matching tasks in this list.")
    : label("no_open_tasks", "No open tasks.");

  const groups = groupTasks(tasks);
  for (const [status, groupedTasks] of groups.entries()) {
    if (groupedTasks.length === 0) {
      continue;
    }

    const section = document.createElement("li");
    section.className = "task-group";

    const details = document.createElement("details");
    details.className = "task-group-details";
    details.open = openStatuses.has(status);

    const summary = document.createElement("summary");
    summary.className = "task-group-summary";
    summary.innerHTML = `
      <span class="task-group-leading">
        <span class="task-group-chevron" aria-hidden="true">▾</span>
      </span>
      <span class="task-group-heading">
        <span class="task-status task-status--${status}">${statusLabel(status)}</span>
        <span class="task-group-count">${groupedTasks.length} ${label("tasks", "tasks")}</span>
      </span>
    `;

    const innerList = document.createElement("ul");
    innerList.className = "task-group-list";
    for (const task of groupedTasks) {
      innerList.appendChild(createTaskItem(task));
    }

    details.append(summary, innerList);
    section.appendChild(details);
    taskList.appendChild(section);
  }
}

refreshButton.addEventListener("click", () => {
  if (searchForm) {
    syncSearchUrl(currentSearchClauses());
  } else {
    const url = new URL(window.location.href);
    const quickFilter = currentQuickFilter();
    url.search = "";
    if (quickFilter) {
      url.searchParams.set("q", quickFilter);
    }
    window.history.replaceState({}, "", url);
  }
  loadTasks().catch(console.error);
});

if (!showQuickFilter) {
  quickFilterInput?.closest(".panel-head-search")?.setAttribute("hidden", "");
}

quickFilterInput?.addEventListener("input", () => {
  if (searchForm) {
    return;
  }
  const url = new URL(window.location.href);
  const quickFilter = currentQuickFilter();
  url.search = "";
  if (quickFilter) {
    url.searchParams.set("q", quickFilter);
  }
  window.history.replaceState({}, "", url);
});

quickFilterInput?.addEventListener("keydown", (event) => {
  if (event.key !== "Enter") {
    return;
  }
  event.preventDefault();
  if (searchForm) {
    syncSearchUrl(currentSearchClauses());
  } else {
    const url = new URL(window.location.href);
    const quickFilter = currentQuickFilter();
    url.search = "";
    if (quickFilter) {
      url.searchParams.set("q", quickFilter);
    }
    window.history.replaceState({}, "", url);
  }
  loadTasks().catch(console.error);
});

initializeSearchForm();
initializeNavDrawer();
Promise.all([
  initializeKnownTags(),
  initializeTagSuggestions(),
]).catch(console.error);
loadTasks().catch(console.error);
