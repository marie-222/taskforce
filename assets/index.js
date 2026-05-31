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

function currentSearchClauses() {
  if (!searchTextarea) {
    return [];
  }

  return searchTextarea.value
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

function syncSearchUrl(clauses) {
  const url = new URL(window.location.href);
  const quickFilter = currentQuickFilter();
  url.search = "";
  if (clauses.length > 0) {
    url.searchParams.set("where", clauses.join("\n"));
  }
  if (quickFilter) {
    url.searchParams.set("q", quickFilter);
  }
  window.history.replaceState({}, "", url);
}

function effectiveApiUrl() {
  if (!searchForm) {
    return apiUrl;
  }

  const clauses = currentSearchClauses();
  const quickFilter = currentQuickFilter();
  if (clauses.length === 0 && searchForm) {
    return null;
  }

  const url = new URL(searchApiBase ?? apiUrl, window.location.origin);
  if (clauses.length > 0) {
    url.searchParams.set("where", clauses.join("\n"));
  }
  if (quickFilter) {
    url.searchParams.set("q", quickFilter);
  }
  return url.toString();
}

function initializeSearchForm() {
  const params = new URLSearchParams(window.location.search);
  const initialQuickFilter = params.get("q");
  if (initialQuickFilter && quickFilterInput) {
    quickFilterInput.value = initialQuickFilter;
  }

  if (!searchForm || !searchTextarea) {
    return;
  }

  const initialWhere = params.get("where");
  if (initialWhere) {
    searchTextarea.value = initialWhere;
  }

  searchForm.addEventListener("submit", (event) => {
    event.preventDefault();
    const clauses = currentSearchClauses();
    syncSearchUrl(clauses);
    loadTasks().catch(console.error);
  });
}

async function loadTasks() {
  taskList.innerHTML = "";
  const requestUrl = effectiveApiUrl();
  if (!requestUrl) {
    emptyState.hidden = false;
    emptyState.textContent = label(
      "search_prompt",
      "Enter at least one WHERE clause."
    );
    return;
  }

  const response = await fetch(requestUrl);
  const tasks = await response.json();
  const filterActive = currentQuickFilter().length > 0;
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
loadTasks().catch(console.error);
