// Copyright (c) 2026- Masaki Ishii
// Copyright (c) 2026- Small Gear Lab
// SPDX-License-Identifier: MIT OR Apache-2.0

const configNode = document.getElementById("taskforce-detail-config");
const config = configNode ? JSON.parse(configNode.textContent ?? "{}") : {};
const labels = config.labels ?? {};
const messages = config.messages ?? {};
const statusLabels = config.status_labels ?? {};
let pluginFields = {};
const rendererModules = new Map();

const taskId = window.location.pathname.split("/").pop();

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

function message(name, fallback) {
  return messages[name] ?? fallback;
}

function textOrFallback(value, fallback = label("dash", "—")) {
  return value == null || value === "" ? fallback : value;
}

function isExternalUrl(value) {
  return typeof value === "string" && /^https?:\/\/\S+$/i.test(value.trim());
}

function isUrlFieldPath(path) {
  return (
    path.endsWith(".url") ||
    path.endsWith("_url")
  );
}

function createExternalLink(url, labelText = url) {
  const link = document.createElement("a");
  link.href = url;
  link.target = "_blank";
  link.rel = "noopener noreferrer";
  link.textContent = labelText;
  link.className = "external-link";
  return link;
}

function createTagLink(tag) {
  const link = document.createElement("a");
  link.href = `/tags/${encodeURIComponent(tag)}`;
  link.className = "tag";
  link.textContent = `#${tag}`;
  return link;
}

function appendLinkifiedText(container, text) {
  const source = text ?? "";
  const urlPattern = /https?:\/\/\S+/gi;
  let lastIndex = 0;

  for (const match of source.matchAll(urlPattern)) {
    const url = match[0];
    const index = match.index ?? 0;
    if (index > lastIndex) {
      container.append(document.createTextNode(source.slice(lastIndex, index)));
    }
    container.append(createExternalLink(url));
    lastIndex = index + url.length;
  }

  if (lastIndex < source.length) {
    container.append(document.createTextNode(source.slice(lastIndex)));
  }
}

function dateLine(date, hint) {
  if (!date && !hint) return label("dash", "—");
  return [date, hint].filter(Boolean).join(" ");
}

function effectiveDescription(task) {
  return task.core.description;
}

function pluginManifest(pluginKey) {
  return pluginFields[pluginKey] ?? null;
}

function pluginEnabled(pluginKey) {
  return pluginManifest(pluginKey) != null;
}

function pluginGroupMeta(pluginKey) {
  return pluginManifest(pluginKey)?.group ?? null;
}

function pluginFieldMeta(path) {
  const [pluginKey, ...rest] = path.split(".");
  const manifest = pluginManifest(pluginKey);
  if (!manifest) {
    return null;
  }
  if (rest.length === 0) {
    return { label: manifest.name };
  }
  return manifest.fields?.[rest.join(".")] ?? null;
}

function pluginFieldEntries(pluginKey) {
  return Object.entries(pluginManifest(pluginKey)?.fields ?? {});
}

function pluginFieldRendererUrl(path) {
  return pluginFieldMeta(path)?.renderer_url ?? null;
}

function fieldPlacement(pluginKey, fieldKey) {
  return pluginManifest(pluginKey)?.fields?.[fieldKey]?.placement ?? "hidden";
}

function hasFieldDescendants(pluginKey, fieldKey, placements) {
  return pluginFieldEntries(pluginKey).some(([candidatePath, meta]) => {
    if (!placements.has(meta?.placement)) {
      return false;
    }
    return (
      candidatePath.startsWith(`${fieldKey}.`) ||
      candidatePath.startsWith(`${fieldKey}[].`)
    );
  });
}

function labelFor(path, fallbackKey) {
  return pluginFieldMeta(path)?.label ?? fallbackKey;
}

async function loadRendererModule(url) {
  if (!url) {
    return null;
  }

  if (!rendererModules.has(url)) {
    rendererModules.set(
      url,
      import(url).catch((error) => {
        console.error(`failed to load plugin renderer: ${url}`, error);
        return null;
      })
    );
  }

  return rendererModules.get(url);
}

async function preloadPluginRenderers() {
  const urls = new Set();
  for (const manifest of Object.values(pluginFields)) {
    for (const field of Object.values(manifest?.fields ?? {})) {
      if (field?.renderer_url) {
        urls.add(field.renderer_url);
      }
    }
  }

  await Promise.all([...urls].map((url) => loadRendererModule(url)));
}

function isObject(value) {
  return value != null && typeof value === "object" && !Array.isArray(value);
}

function filterPluginFields(pluginKey, pluginValue, placements) {
  function hasScalarValue(value) {
    return value !== null && value !== undefined && value !== "";
  }

  function filterValue(fieldKey, value) {
    const placement = fieldPlacement(pluginKey, fieldKey);

    if (Array.isArray(value)) {
      const items = value
        .map((item) => {
          if (isObject(item)) {
            const filteredItem = Object.fromEntries(
              Object.entries(item)
                .map(([childKey, childValue]) => [
                  childKey,
                  filterValue(`${fieldKey}[].${childKey}`, childValue),
                ])
                .filter(([, childValue]) => childValue !== undefined)
            );
            return Object.keys(filteredItem).length > 0 ? filteredItem : undefined;
          }

          return placements.has(fieldPlacement(pluginKey, `${fieldKey}[]`)) &&
            hasScalarValue(item)
            ? item
            : undefined;
        })
        .filter((item) => item !== undefined);

      if (placements.has(placement) || hasFieldDescendants(pluginKey, fieldKey, placements)) {
        return items;
      }
      return undefined;
    }

    if (isObject(value)) {
      const filteredObject = Object.fromEntries(
        Object.entries(value)
          .map(([childKey, childValue]) => [
            childKey,
            filterValue(`${fieldKey}.${childKey}`, childValue),
          ])
          .filter(([, childValue]) => childValue !== undefined)
      );

      if (placements.has(placement) || Object.keys(filteredObject).length > 0) {
        return filteredObject;
      }
      return undefined;
    }

    return placements.has(placement) && hasScalarValue(value)
      ? value
      : undefined;
  }

  if (isObject(pluginValue)) {
    return Object.fromEntries(
      Object.entries(pluginValue)
        .map(([fieldKey, value]) => [fieldKey, filterValue(fieldKey, value)])
        .filter(([, value]) => value !== undefined)
    );
  }

  return undefined;
}

function normalizePluginExtra(extra, placements = new Set(["right"])) {
  if (!isObject(extra)) {
    return {};
  }

  function hasVisibleContent(value) {
    if (Array.isArray(value)) {
      return value.length > 0;
    }
    if (isObject(value)) {
      return Object.keys(value).length > 0;
    }
    return value !== undefined;
  }

  const namespaces = {};
  for (const [key, value] of Object.entries(extra)) {
    if (!pluginEnabled(key)) {
      continue;
    }
    const filteredValue = filterPluginFields(key, value, placements);
    if (hasVisibleContent(filteredValue)) {
      namespaces[key] = filteredValue;
    }
  }

  return namespaces;
}

function previewValue(value) {
  if (Array.isArray(value)) {
    return `${value.length} items`;
  }
  if (value && typeof value === "object") {
    return `${Object.keys(value).length} fields`;
  }
  if (value == null) {
    return label("dash", "—");
  }
  const text = String(value);
  return text.length > 52 ? `${text.slice(0, 52)}…` : text;
}

function renderJsonTree(path, key, value) {
  if (Array.isArray(value)) {
    if (key === "extra") {
      const wrapper = document.createElement("div");
      wrapper.className = "tree-root";
      value.forEach((item, index) => {
        wrapper.appendChild(renderJsonTree(`${path}[]`, `[${index}]`, item));
      });
      if (value.length === 0) {
        const empty = document.createElement("div");
        empty.className = "empty";
        empty.textContent = message("no_extra_data", "No extra data.");
        wrapper.appendChild(empty);
      }
      return wrapper;
    }

    const details = document.createElement("details");
    details.open = false;
    const summary = document.createElement("summary");
    summary.innerHTML = `
      <span class="tree-key">${labelFor(path, key)}</span>
      <span class="tree-preview">${previewValue(value)}</span>
    `;
    details.appendChild(summary);

    const content = document.createElement("div");
    content.className = "tree-content tree-children";
    if (value.length === 0) {
      const empty = document.createElement("div");
      empty.className = "empty";
      empty.textContent = message("no_extra_data", "No extra data.");
      content.appendChild(empty);
    } else {
      value.forEach((item, index) => {
        content.appendChild(renderJsonTree(`${path}[]`, `[${index}]`, item));
      });
    }
    details.appendChild(content);

    const wrapper = document.createElement("div");
    wrapper.className = "tree-node tree-node--branch";
    wrapper.appendChild(details);
    return wrapper;
  }

  if (value && typeof value === "object") {
    if (key === "extra") {
      const wrapper = document.createElement("div");
      wrapper.className = "tree-root";
      const entries = Object.entries(value);
      if (entries.length === 0) {
        const empty = document.createElement("div");
        empty.className = "empty";
        empty.textContent = message("no_extra_data", "No extra data.");
        wrapper.appendChild(empty);
      } else {
        for (const [childKey, childValue] of entries) {
          wrapper.appendChild(renderJsonTree(childKey, childKey, childValue));
        }
      }
      return wrapper;
    }

      const details = document.createElement("details");
      details.open = false;
    const summary = document.createElement("summary");
    summary.innerHTML = `
      <span class="tree-key">${labelFor(path, key)}</span>
      <span class="tree-preview">${previewValue(value)}</span>
    `;
    details.appendChild(summary);

    const content = document.createElement("div");
    content.className = "tree-content tree-children";
    const entries = Object.entries(value);
    if (entries.length === 0) {
      const empty = document.createElement("div");
      empty.className = "empty";
      empty.textContent = message("no_extra_data", "No extra data.");
      content.appendChild(empty);
    } else {
      for (const [childKey, childValue] of entries) {
        content.appendChild(renderJsonTree(`${path}.${childKey}`, childKey, childValue));
      }
    }
    details.appendChild(content);

    const wrapper = document.createElement("div");
    wrapper.className = "tree-node tree-node--branch";
    wrapper.appendChild(details);
    return wrapper;
  }

  const leaf = document.createElement("div");
  leaf.className = "tree-node tree-node--leaf tree-leaf";
  const keyNode = document.createElement("div");
  keyNode.className = "tree-key";
  keyNode.textContent = labelFor(path, key);
  leaf.appendChild(keyNode);

  const valueNode = document.createElement("div");
  valueNode.className = "tree-leaf-value";
  const textValue = textOrFallback(value);
  if (isUrlFieldPath(path) && isExternalUrl(textValue)) {
    valueNode.appendChild(createExternalLink(textValue));
  } else {
    valueNode.textContent = textValue;
  }
  leaf.appendChild(valueNode);
  return leaf;
}

function wireExtraViewToggle(treeButton, rawButton, expandAllButton, collapseAllButton, treeView, rawView) {
  function setMode(mode) {
    const treeActive = mode === "tree";
    treeButton.classList.toggle("is-active", treeActive);
    rawButton.classList.toggle("is-active", !treeActive);
    treeView.hidden = !treeActive;
    rawView.hidden = treeActive;
  }

  treeButton.addEventListener("click", () => setMode("tree"));
  rawButton.addEventListener("click", () => setMode("raw"));
  expandAllButton.addEventListener("click", () => {
    treeView.querySelectorAll("details").forEach((node) => {
      node.open = true;
    });
    setMode("tree");
  });
  collapseAllButton.addEventListener("click", () => {
    treeView.querySelectorAll("details").forEach((node) => {
      node.open = false;
    });
    setMode("tree");
  });
  setMode("tree");
}

async function renderFieldValueNode(pluginKey, path, key, value, task) {
  const rendererUrl = pluginFieldRendererUrl(path);
  if (!rendererUrl) {
    return renderJsonTree(path, key, value);
  }

  const rendererModule = await loadRendererModule(rendererUrl);
  if (typeof rendererModule?.render !== "function") {
    return renderJsonTree(path, key, value);
  }

  try {
    const rendered = await rendererModule.render(value, {
      fieldPath: path,
      fieldKey: key,
      pluginId: pluginKey,
      task,
      message,
      helpers: {
        appendLinkifiedText,
        createExternalLink,
        isExternalUrl,
        textOrFallback,
      },
    });
    if (!(rendered instanceof Node)) {
      return renderJsonTree(path, key, value);
    }

    const wrapper = document.createElement("div");
    wrapper.className = "tree-node tree-node--leaf tree-node--rendered";

    const keyNode = document.createElement("div");
    keyNode.className = "tree-key";
    keyNode.textContent = labelFor(path, key);
    wrapper.appendChild(keyNode);

    const valueNode = document.createElement("div");
    valueNode.className = "tree-leaf-value tree-leaf-value--rendered";
    valueNode.appendChild(rendered);
    wrapper.appendChild(valueNode);

    return wrapper;
  } catch (error) {
    console.error(`failed to render plugin field ${path}`, error);
    return renderJsonTree(path, key, value);
  }
}

async function renderPluginExtraSection(pluginKey, pluginValue, task) {
  const section = document.createElement("div");
  section.className = "plugin-section";

  const details = document.createElement("details");
  details.open = true;

  const summary = document.createElement("summary");
  summary.className = "plugin-section__summary";
  summary.innerHTML = `<span class="kv-key">${labelFor(pluginKey, pluginKey)}</span>`;
  details.appendChild(summary);

  const content = document.createElement("div");
  content.className = "plugin-section__content";

  const tools = document.createElement("div");
  tools.className = "extra-tools";
  tools.innerHTML = `
    <div class="view-toggle" role="tablist" aria-label="${pluginKey}-data-view">
      <button class="is-active" type="button">${label("tree", "Tree")}</button>
      <button type="button">${label("raw", "Raw")}</button>
    </div>
    <button type="button">${label("expand_all", "Expand all")}</button>
    <button type="button">${label("collapse_all", "Collapse all")}</button>
  `;
  content.appendChild(tools);

  const treeView = document.createElement("div");
  treeView.className = "extra-view tree-root";
  if (pluginValue && typeof pluginValue === "object" && !Array.isArray(pluginValue)) {
    const entries = Object.entries(pluginValue);
    if (entries.length === 0) {
      const empty = document.createElement("div");
      empty.className = "empty";
      empty.textContent = message("no_extra_data", "No extra data.");
      treeView.appendChild(empty);
    } else {
      for (const [childKey, childValue] of entries) {
        treeView.appendChild(
          await renderFieldValueNode(
            pluginKey,
            `${pluginKey}.${childKey}`,
            childKey,
            childValue,
            task
          )
        );
      }
    }
  } else {
    treeView.appendChild(
      await renderFieldValueNode(pluginKey, pluginKey, pluginKey, pluginValue, task)
    );
  }
  content.appendChild(treeView);

  const rawView = document.createElement("pre");
  rawView.className = "extra-view";
  rawView.hidden = true;
  rawView.textContent = JSON.stringify(pluginValue, null, 2);
  content.appendChild(rawView);

  details.appendChild(content);
  section.appendChild(details);

  wireExtraViewToggle(
    tools.querySelector(".view-toggle button:first-child"),
    tools.querySelector(".view-toggle button:last-child"),
    tools.querySelectorAll("button")[2],
    tools.querySelectorAll("button")[3],
    treeView,
    rawView
  );

  return section;
}

async function renderGroupedPluginExtraSection(groupId, groupLabel, entries, task) {
  const section = document.createElement("div");
  section.className = "plugin-section";

  const details = document.createElement("details");
  details.open = true;

  const summary = document.createElement("summary");
  summary.className = "plugin-section__summary";
  summary.innerHTML = `<span class="kv-key">${groupLabel}</span>`;
  details.appendChild(summary);

  const content = document.createElement("div");
  content.className = "plugin-section__content";

  const tools = document.createElement("div");
  tools.className = "extra-tools";
  tools.innerHTML = `
    <div class="view-toggle" role="tablist" aria-label="${groupId}-data-view">
      <button class="is-active" type="button">${label("tree", "Tree")}</button>
      <button type="button">${label("raw", "Raw")}</button>
    </div>
    <button type="button">${label("expand_all", "Expand all")}</button>
    <button type="button">${label("collapse_all", "Collapse all")}</button>
  `;
  content.appendChild(tools);

  const treeView = document.createElement("div");
  treeView.className = "extra-view tree-root";

  let renderedAny = false;
  for (const [pluginKey, pluginValue] of entries) {
    if (pluginValue && typeof pluginValue === "object" && !Array.isArray(pluginValue)) {
      const fields = Object.entries(pluginValue);
      for (const [childKey, childValue] of fields) {
        treeView.appendChild(
          await renderFieldValueNode(
            pluginKey,
            `${pluginKey}.${childKey}`,
            childKey,
            childValue,
            task
          )
        );
        renderedAny = true;
      }
    } else {
      treeView.appendChild(
        await renderFieldValueNode(pluginKey, pluginKey, pluginKey, pluginValue, task)
      );
      renderedAny = true;
    }
  }

  if (!renderedAny) {
    const empty = document.createElement("div");
    empty.className = "empty";
    empty.textContent = message("no_extra_data", "No extra data.");
    treeView.appendChild(empty);
  }
  content.appendChild(treeView);

  const rawView = document.createElement("pre");
  rawView.className = "extra-view";
  rawView.hidden = true;
  rawView.textContent = JSON.stringify(Object.fromEntries(entries), null, 2);
  content.appendChild(rawView);

  details.appendChild(content);
  section.appendChild(details);

  wireExtraViewToggle(
    tools.querySelector(".view-toggle button:first-child"),
    tools.querySelector(".view-toggle button:last-child"),
    tools.querySelectorAll("button")[2],
    tools.querySelectorAll("button")[3],
    treeView,
    rawView
  );

  return section;
}

async function renderPluginExtraSections(
  container,
  extra,
  placements = new Set(["right"]),
  { showEmpty = true, task = null } = {}
) {
  const namespaces = Object.entries(normalizePluginExtra(extra, placements));

  if (namespaces.length === 0) {
    if (showEmpty) {
      const empty = document.createElement("div");
      empty.className = "empty";
      empty.textContent = message("no_extra_data", "No extra data.");
      container.appendChild(empty);
    }
    return 0;
  }

  const grouped = new Map();

  for (const [pluginKey, pluginValue] of namespaces) {
    const group = pluginGroupMeta(pluginKey);
    if (!group) {
      container.appendChild(await renderPluginExtraSection(pluginKey, pluginValue, task));
      continue;
    }

    if (!grouped.has(group.id)) {
      grouped.set(group.id, {
        label: group.label ?? group.id,
        entries: [],
      });
    }
    grouped.get(group.id).entries.push([pluginKey, pluginValue]);
  }

  for (const [groupId, groupValue] of grouped.entries()) {
    container.appendChild(
      await renderGroupedPluginExtraSection(
        groupId,
        groupValue.label,
        groupValue.entries,
        task
      )
    );
  }

  return namespaces.length;
}

async function loadTask() {
  const [taskResponse, pluginResponse] = await Promise.all([
    fetch(`/api/tasks/${taskId}`),
    fetch("/api/plugin-manifests"),
  ]);

  if (!taskResponse.ok) {
    document.getElementById("task-title").textContent = message("task_not_found", "Task not found");
    document.getElementById("task-description-section").hidden = false;
    document.getElementById("task-description").textContent = message(
      "task_could_not_be_loaded",
      "The requested task could not be loaded."
    );
    return;
  }

  pluginFields = pluginResponse.ok ? await pluginResponse.json() : {};
  await preloadPluginRenderers();

  const task = await taskResponse.json();
  const metaLine = document.getElementById("meta-line");
  const schedule = document.getElementById("schedule");
  const scheduleSection = document.getElementById("task-schedule-section");
  const projectTagsSection = document.getElementById("task-project-tags-section");
  const projectRow = document.getElementById("task-project-row");
  const tagsRow = document.getElementById("task-tags-row");
  const tagList = document.getElementById("tag-list");
  const pluginLeftSection = document.getElementById("task-plugin-left-section");
  const pluginLeftSections = document.getElementById("plugin-left-sections");
  const pluginExtraSections = document.getElementById("plugin-extra-sections");
  const descriptionSection = document.getElementById("task-description-section");
  const description = effectiveDescription(task);

  document.title = `${task.core.title} | taskforce`;
  document.getElementById("task-title").textContent = task.core.title;
  if (description) {
    descriptionSection.hidden = false;
    document.getElementById("task-description").textContent = description;
  } else {
    descriptionSection.hidden = true;
    document.getElementById("task-description").textContent = "";
  }
  pluginLeftSections.innerHTML = "";
  pluginLeftSection.hidden =
    await renderPluginExtraSections(
      pluginLeftSections,
      task.extra,
      new Set(["left"]),
      { showEmpty: false, task }
    ) === 0;
  projectRow.hidden = !task.core.project;
  document.getElementById("project-value").textContent = task.core.project ?? "";
  pluginExtraSections.innerHTML = "";
  await renderPluginExtraSections(pluginExtraSections, task.extra, new Set(["right"]), { task });

  metaLine.innerHTML = "";
  for (const chipText of [
    `#${task.id ?? "?"}`,
    statusLabels[task.core.status] ?? task.core.status,
    task.core.project ?? message("no_project", "no project"),
    `${task.annotations?.length ?? 0} ${label("annotations", "annotations")}`
  ]) {
    const chip = document.createElement("span");
    chip.className = "chip";
    chip.textContent = chipText;
    metaLine.appendChild(chip);
  }

  schedule.innerHTML = "";
  let scheduleCount = 0;
  for (const [name, value] of [
    ["target", dateLine(task.core.target_date, task.core.target_time_hint)],
    ["deadline", dateLine(task.core.deadline, task.core.deadline_time_hint)],
    ["launch", dateLine(task.core.launch_date, task.core.launch_time_hint)]
  ]) {
    if (value === label("dash", "—")) {
      continue;
    }
    const row = document.createElement("div");
    row.className = "schedule-row";
    row.innerHTML = `
      <div class="schedule-label">${label(name, name)}</div>
      <div class="schedule-value">${value}</div>
    `;
    schedule.appendChild(row);
    scheduleCount += 1;
  }
  scheduleSection.hidden = scheduleCount === 0;

  tagList.innerHTML = "";
  tagsRow.hidden = (task.core.tags ?? []).length === 0;
  if ((task.core.tags ?? []).length > 0) {
    for (const tag of task.core.tags) {
      tagList.appendChild(createTagLink(tag));
    }
  }
  projectTagsSection.hidden = projectRow.hidden && tagsRow.hidden;
}

initializeNavDrawer();
loadTask().catch(console.error);
