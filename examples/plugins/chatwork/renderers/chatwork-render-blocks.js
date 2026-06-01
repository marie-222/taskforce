function isExternalUrl(value) {
  return typeof value === "string" && /^https?:\/\/\S+$/i.test(value.trim());
}

function appendLinkifiedText(container, text, helpers) {
  const source = text ?? "";
  const urlPattern = /https?:\/\/\S+/gi;
  let lastIndex = 0;

  for (const match of source.matchAll(urlPattern)) {
    const url = match[0];
    const index = match.index ?? 0;
    if (index > lastIndex) {
      container.append(document.createTextNode(source.slice(lastIndex, index)));
    }
    container.append(helpers.createExternalLink(url));
    lastIndex = index + url.length;
  }

  if (lastIndex < source.length) {
    container.append(document.createTextNode(source.slice(lastIndex)));
  }
}

function pushTextRenderBlock(blocks, text) {
  const normalized = text.trim();
  if (!normalized) {
    return;
  }

  blocks.push({
    kind: "text",
    title: null,
    text: normalized,
    children: [],
  });
}

function findNextMarkup(text, terminator) {
  const indexes = [];
  for (const marker of ["[info]", "[code]", "[qt]", "[hr]"]) {
    const markerIndex = text.indexOf(marker);
    if (markerIndex >= 0) {
      indexes.push(markerIndex);
    }
  }
  if (terminator) {
    const terminatorIndex = text.indexOf(terminator);
    if (terminatorIndex >= 0) {
      indexes.push(terminatorIndex);
    }
  }

  return indexes.length === 0 ? text.length : Math.min(...indexes);
}

function parseRenderBlocks(text, terminator) {
  const blocks = [];
  let index = 0;

  while (index < text.length) {
    const rest = text.slice(index);

    if (terminator && rest.startsWith(terminator)) {
      return { blocks, index };
    }

    if (rest.startsWith("[info]")) {
      const parsed = parseInfoBlock(rest);
      blocks.push(parsed.block);
      index += parsed.consumed;
      continue;
    }

    if (rest.startsWith("[code]")) {
      const parsed = parseCodeBlock(rest);
      blocks.push(parsed.block);
      index += parsed.consumed;
      continue;
    }

    if (rest.startsWith("[qt]")) {
      const parsed = parseQuoteBlock(rest);
      blocks.push(parsed.block);
      index += parsed.consumed;
      continue;
    }

    if (rest.startsWith("[hr]")) {
      blocks.push({
        kind: "rule",
        title: null,
        text: "",
        children: [],
      });
      index += "[hr]".length;
      continue;
    }

    const nextIndex = findNextMarkup(rest, terminator);
    pushTextRenderBlock(blocks, rest.slice(0, nextIndex));
    index += nextIndex;
  }

  return { blocks, index };
}

function parseInfoBlock(text) {
  let index = "[info]".length;
  while (index < text.length && /\s/.test(text[index])) {
    index += 1;
  }

  let title = null;
  if (text.slice(index).startsWith("[title]")) {
    const closeIndex = text.indexOf("[/title]", index + "[title]".length);
    if (closeIndex >= 0) {
      const titleStart = index + "[title]".length;
      title = text.slice(titleStart, closeIndex).trim();
      index = closeIndex + "[/title]".length;
    }
  }

  const parsed = parseRenderBlocks(text.slice(index), "[/info]");
  const children = [...parsed.blocks];
  let bodyText = "";
  if (children[0]?.kind === "text") {
    bodyText = children.shift().text ?? "";
  }

  const closeOffset = index + parsed.index;
  const consumed = text.slice(closeOffset).startsWith("[/info]")
    ? closeOffset + "[/info]".length
    : text.length;

  return {
    block: {
      kind: "info",
      title,
      text: bodyText,
      children,
    },
    consumed,
  };
}

function parseCodeBlock(text) {
  const closeIndex = text.indexOf("[/code]", "[code]".length);
  if (closeIndex >= 0) {
    return {
      block: {
        kind: "code",
        title: null,
        text: text.slice("[code]".length, closeIndex).trim(),
        children: [],
      },
      consumed: closeIndex + "[/code]".length,
    };
  }

  return {
    block: {
      kind: "code",
      title: null,
      text: text.trim(),
      children: [],
    },
    consumed: text.length,
  };
}

function parseQuoteBlock(text) {
  const innerStart = "[qt]".length;
  const parsed = parseRenderBlocks(text.slice(innerStart), "[/qt]");
  const children = [...parsed.blocks];
  let bodyText = "";
  if (children[0]?.kind === "text") {
    bodyText = children.shift().text ?? "";
  }

  const closeOffset = innerStart + parsed.index;
  const consumed = text.slice(closeOffset).startsWith("[/qt]")
    ? closeOffset + "[/qt]".length
    : text.length;

  return {
    block: {
      kind: "quote",
      title: null,
      text: bodyText,
      children,
    },
    consumed,
  };
}

function normalizeBlocks(value, context) {
  if (Array.isArray(value) && value.length > 0) {
    return value;
  }

  if (typeof value === "string" && value.trim() !== "") {
    return parseRenderBlocks(value, null).blocks;
  }

  const bodyRaw = context.task?.extra?.[context.pluginId]?.source?.body_raw;
  if (typeof bodyRaw === "string" && bodyRaw.trim() !== "") {
    return parseRenderBlocks(bodyRaw, null).blocks;
  }

  return [];
}

function renderRequestBlock(block, helpers) {
  if (block.kind === "rule") {
    const rule = document.createElement("hr");
    rule.className = "request-block request-block--rule";
    return rule;
  }

  if (block.kind === "quote") {
    const quote = document.createElement("blockquote");
    quote.className = "request-block request-block--quote";
    if (block.text) {
      appendLinkifiedText(quote, block.text, helpers);
    }
    if (Array.isArray(block.children) && block.children.length > 0) {
      const children = document.createElement("div");
      children.className = "request-block__children";
      for (const child of block.children) {
        children.appendChild(renderRequestBlock(child, helpers));
      }
      quote.appendChild(children);
    }
    return quote;
  }

  if (block.kind === "code") {
    const pre = document.createElement("pre");
    pre.className = "request-block request-block--code";
    pre.textContent = block.text ?? "";
    return pre;
  }

  if (block.kind === "info") {
    const wrapper = document.createElement("section");
    wrapper.className = "request-block request-block--info";
    if (block.title) {
      const title = document.createElement("div");
      title.className = "request-block__title";
      title.textContent = block.title;
      wrapper.appendChild(title);
    }
    if (block.text) {
      const body = document.createElement("div");
      body.className = "request-block__body";
      appendLinkifiedText(body, block.text, helpers);
      wrapper.appendChild(body);
    }
    if (Array.isArray(block.children) && block.children.length > 0) {
      const children = document.createElement("div");
      children.className = "request-block__children";
      for (const child of block.children) {
        children.appendChild(renderRequestBlock(child, helpers));
      }
      wrapper.appendChild(children);
    }
    return wrapper;
  }

  const paragraph = document.createElement("div");
  paragraph.className = "request-block request-block--text section-body";
  appendLinkifiedText(paragraph, block.text ?? "", helpers);
  return paragraph;
}

export function render(value, context) {
  const helpers = context.helpers ?? {};
  const blocks = normalizeBlocks(value, context);
  const root = document.createElement("div");

  if (blocks.length === 0) {
    root.className = "section-body empty";
    root.textContent = context.message(
      "no_original_request",
      "Original request text is not available."
    );
    return root;
  }

  root.className = "section-stack";
  for (const block of blocks) {
    root.appendChild(renderRequestBlock(block, helpers));
  }
  return root;
}
