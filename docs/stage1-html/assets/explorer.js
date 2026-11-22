(function () {
  "use strict";
  const host = document.getElementById("explorer");
  if (!host) return;
  const graph = window.STAGE1_CALL_GRAPH;
  if (!graph || !Array.isArray(graph.nodes) || !Array.isArray(graph.edges)) {
    host.textContent = "Call-flow data is unavailable. Load assets/call-flow.js before assets/explorer.js.";
    return;
  }

  const nodes = new Map(graph.nodes.map(node => [node.id, node]));
  const incoming = new Map(graph.nodes.map(node => [node.id, []]));
  const outgoing = new Map(graph.nodes.map(node => [node.id, []]));
  for (const edge of graph.edges) {
    if (!nodes.has(edge.from) || !nodes.has(edge.to)) continue;
    outgoing.get(edge.from).push(edge);
    incoming.get(edge.to).push(edge);
  }
  const state = { selected: nodes.has("main") ? "main" : graph.nodes[0]?.id, depth: 1, filter: "all", query: "", path: [], guided: null, guidedIndex: 0 };
  const guidedPaths = Array.isArray(window.STAGE1_GUIDED_PATHS) ? window.STAGE1_GUIDED_PATHS : [];
  host.classList.add("cf-explorer");

  function element(tag, className, text) {
    const el = document.createElement(tag);
    if (className) el.className = className;
    if (text !== undefined) el.textContent = text;
    return el;
  }
  function button(label, fn, className) {
    const el = element("button", className, label);
    el.type = "button";
    el.addEventListener("click", fn);
    return el;
  }
  function source(text) {
    const code = element("code", "cf-source", text);
    return code;
  }
  function labelled(label, value) {
    const p = element("p", "cf-field");
    p.append(element("strong", "", label + " "), element("span", "", value));
    return p;
  }
  function clearGuided() {
    state.guided = null;
    state.guidedIndex = 0;
    if (guided) guided.value = "free";
  }
  function setSelected(id) {
    if (!nodes.has(id)) return;
    if (state.guided) clearGuided();
    state.selected = id;
    state.path = [];
    render();
  }
  function pathTo(id) {
    if (state.guided) clearGuided();
    if (id === state.selected) { state.path = []; render(); return; }
    const queue = [state.selected];
    const previous = new Map([[state.selected, null]]);
    for (let index = 0; index < queue.length && !previous.has(id); index++) {
      for (const edge of outgoing.get(queue[index])) {
        if (!previous.has(edge.to)) {
          previous.set(edge.to, edge);
          queue.push(edge.to);
        }
      }
    }
    if (!previous.has(id)) {
      status.textContent = "No directed path from " + state.selected + " to " + id + ". Select another start node or follow incoming links.";
      return;
    }
    const path = [];
    for (let cursor = id; cursor !== state.selected;) {
      const edge = previous.get(cursor);
      path.unshift(edge);
      cursor = edge.from;
    }
    state.path = path;
    state.depth = Math.max(state.depth, Math.min(4, path.length));
    depth.value = String(state.depth);
    render();
  }
  function matches(node) {
    return (state.filter === "all" || node.subsystem === state.filter) &&
      (!state.query || [node.id, node.signature, node.purpose, node.module, node.path]
        .some(value => value.toLowerCase().includes(state.query)));
  }
  function neighborhood() {
    const distance = new Map([[state.selected, 0]]);
    const queue = [state.selected];
    for (let index = 0; index < queue.length; index++) {
      const id = queue[index];
      const depth = distance.get(id);
      if (depth >= state.depth) continue;
      for (const edge of incoming.get(id).concat(outgoing.get(id))) {
        const other = edge.from === id ? edge.to : edge.from;
        if (!distance.has(other)) { distance.set(other, depth + 1); queue.push(other); }
      }
    }
    return distance;
  }

  const heading = element("h2", "cf-heading", "Explore the call flow");
  const intro = element("p", "cf-intro", "Select a function or labeled external-tool boundary, expand its neighborhood up to four hops, or follow callers and callees. Direct calls, subprocess communication, data handoffs and conceptual navigation are distinct; source lines are evidence, not web URLs.");
  const controls = element("div", "cf-controls");
  const searchGroup = element("label", "cf-control", "Search functions");
  const search = element("input");
  search.type = "search";
  search.placeholder = "Function, path, purpose…";
  search.autocomplete = "off";
  searchGroup.append(search);
  const filterGroup = element("label", "cf-control", "Subsystem");
  const filter = element("select");
  filter.append(element("option", "", "All subsystems"));
  filter.firstChild.value = "all";
  for (const [key, name] of Object.entries(graph.subsystems)) {
    const option = element("option", "", name);
    option.value = key;
    filter.append(option);
  }
  filterGroup.append(filter);
  const depthGroup = element("label", "cf-control", "Neighborhood depth");
  const depth = element("select");
  for (let i = 1; i <= 4; i++) {
    const option = element("option", "", `${i} ${i === 1 ? "hop" : "hops"}`);
    option.value = String(i);
    depth.append(option);
  }
  depthGroup.append(depth);
  controls.append(searchGroup, filterGroup, depthGroup);
  const guidedGroup = element("label", "cf-control", "Guided path");
  const guided = element("select");
  const freeOption = element("option", "", "Free exploration");
  freeOption.value = "free";
  guided.append(freeOption);
  for (const path of guidedPaths) {
    const option = element("option", "", path.title);
    option.value = path.id;
    guided.append(option);
  }
  guidedGroup.append(guided);
  controls.append(guidedGroup);
  const guidedPanel = element("section", "cf-guided");
  guidedPanel.hidden = true;
  guidedPanel.setAttribute("aria-label", "Guided execution path");
  const guidedTitle = element("h3", "", "Guided path");
  const guidedSummary = element("p", "cf-guided-summary");
  const guidedSteps = element("div", "cf-guided-steps");
  const guidedNav = element("div", "cf-guided-nav");
  guidedPanel.append(guidedTitle, guidedSummary, guidedSteps, guidedNav);

  const status = element("p", "cf-status");
  status.setAttribute("role", "status");
  status.setAttribute("aria-live", "polite");
  const layout = element("div", "cf-layout");
  const listSection = element("section", "cf-list-section");
  listSection.setAttribute("aria-label", "Matching functions and external interfaces");
  const listHeading = element("h3", "", "Nodes");
  const list = element("div", "cf-list");
  listSection.append(listHeading, list);
  const mainSection = element("div", "cf-main");
  const mapSection = element("section", "cf-map-section");
  mapSection.setAttribute("aria-label", "Multi-hop function neighborhood");
  const mapHeading = element("h3", "", "Neighborhood");
  const map = element("div", "cf-map");
  mapSection.append(mapHeading, map);
  const detailSection = element("section", "cf-detail");
  detailSection.setAttribute("aria-label", "Selected function details");
  mainSection.append(mapSection, detailSection);
  layout.append(listSection, mainSection);
  host.append(heading, intro, controls, guidedPanel, status, layout);

  function renderList() {
    const matchesList = graph.nodes.filter(matches);
    listHeading.textContent = `Nodes (${matchesList.length})`;
    const fragment = document.createDocumentFragment();
    for (const node of matchesList) {
      const row = element("div", "cf-list-row");
      const item = button(node.id, () => setSelected(node.id), "cf-list-item");
      item.append(element("small", "", graph.subsystems[node.subsystem] + " · " + node.module));
      if (node.id === state.selected) item.setAttribute("aria-current", "true");
      row.append(item);
      if (node.id !== state.selected) row.append(button("Path", () => pathTo(node.id), "cf-list-path"));
      fragment.append(row);
    }
    if (!matchesList.length) fragment.append(element("p", "cf-empty", "No nodes match. Clear search or change subsystem."));
    list.replaceChildren(fragment);
  }
  function renderMap() {
    const distance = neighborhood();
    if (state.guided) {
      state.guided.nodes.forEach((step, index) => {
        if (!distance.has(step.id)) distance.set(step.id, index);
      });
    }
    const visible = new Set([...distance.keys()].filter(id => id === state.selected || matches(nodes.get(id))));
    for (const edge of state.path) { visible.add(edge.from); visible.add(edge.to); }
    const fragment = document.createDocumentFragment();
    const pathNodes = new Set(state.path.flatMap(edge => [edge.from, edge.to]));
    if (state.path.length) {
      const route = element("div", "cf-route");
      route.append(element("h4", "", `Directed path (${state.path.length} relationships)`));
      for (const edge of state.path) {
        const step = element("div", "cf-route-step");
        step.append(button(edge.from, () => setSelected(edge.from)), element("span", "cf-arrow", "→"),
          button(edge.to, () => setSelected(edge.to)), element("span", `cf-kind cf-kind-${edge.kind}`, edge.kind));
        route.append(step);
      }
      fragment.append(route);
    }
    for (let tier = 0; tier <= state.depth; tier++) {
      const ids = [...visible].filter(id => distance.get(id) === tier);
      if (!ids.length) continue;
      const group = element("div", "cf-tier");
      group.append(element("h4", "", tier === 0 ? "Selected" : `Hop ${tier}`));
      const cards = element("div", "cf-cards");
      for (const id of ids) {
        const node = nodes.get(id);
        const card = button(node.id, () => setSelected(id), "cf-card");
        if (id === state.selected) card.setAttribute("aria-current", "true");
        if (pathNodes.has(id)) card.classList.add("cf-on-path");
        card.append(element("small", "", graph.subsystems[node.subsystem]), element("span", "", node.purpose));
        cards.append(card);
      }
      group.append(cards);
      fragment.append(group);
    }
    const shownEdges = graph.edges.filter(edge => visible.has(edge.from) && visible.has(edge.to));
    if (shownEdges.length) {
      const connections = element("div", "cf-connections");
      connections.append(element("h4", "", `Connections (${shownEdges.length})`));
      const edgeList = element("ul");
      for (const edge of shownEdges) {
        const item = element("li", "cf-connection");
        if (state.path.includes(edge)) item.classList.add("cf-on-path");
        item.append(button(edge.from, () => setSelected(edge.from)), element("span", "cf-arrow", "→"),
          button(edge.to, () => setSelected(edge.to)), element("span", `cf-kind cf-kind-${edge.kind}`, edge.kind));
        edgeList.append(item);
      }
      connections.append(edgeList);
      fragment.append(connections);
    }
    map.replaceChildren(fragment);
    status.textContent = `${graph.nodes.length} nodes · ${graph.edges.length} relationships · ${visible.size} visible in ${state.depth} ${state.depth === 1 ? "hop" : "hops"}. ` +
      (state.guided ? `Guided path: ${state.guided.title} (step ${state.guidedIndex + 1}/${state.guided.nodes.length}).` : state.path.length ? `Highlighted directed path: ${state.path.map(edge => edge.from).concat(state.path.at(-1).to).join(" → ")}.` : "Use details to highlight a directed path.");
  }
  function relationshipSection(title, edges, direction) {
    const section = element("div", "cf-relations");
    section.append(element("h4", "", `${title} (${edges.length})`));
    if (!edges.length) section.append(element("p", "cf-empty", "None in this map."));
    for (const edge of edges) {
      const row = element("div", "cf-relation");
      const other = direction === "out" ? edge.to : edge.from;
      row.append(button(other, () => setSelected(other), "cf-peer"), element("span", `cf-kind cf-kind-${edge.kind}`, edge.kind));
      if (direction === "out") row.append(button("Highlight path", () => pathTo(other), "cf-path-button"));
      row.append(element("p", "", edge.explanation), labelled("Evidence:", edge.evidence));
      section.append(row);
    }
    return section;
  }
  function renderDetail() {
    const node = nodes.get(state.selected);
    if (!node) return;
    const fragment = document.createDocumentFragment();
    fragment.append(element("h3", "", node.id), element("p", "cf-signature", node.signature));
    fragment.append(labelled("Kind:", node.kind), labelled("Subsystem:", graph.subsystems[node.subsystem]), labelled("Module:", node.module),
      labelled("Purpose:", node.purpose), labelled("Inputs / outputs:", node.inputsOutputs), labelled("Evidence boundary:", node.state));
    const sources = element("div", "cf-source-row");
    sources.append(element("strong", "", "Source: "), source(node.source));
    const sourceLink = element("a", "cf-doc", "Open local source");
    sourceLink.href = "../../" + node.path;
    sources.append(sourceLink);
    const copy = button("Copy path:line", () => {
      const fallback = () => {
        const field = element("textarea");
        field.value = node.source;
        field.style.position = "fixed";
        field.style.opacity = "0";
        host.append(field);
        field.select();
        let copied = false;
        try { copied = document.execCommand("copy"); } catch (_) { /* file:// may restrict clipboard */ }
        field.remove();
        if (!copied) {
          const selection = element("input");
          selection.value = node.source;
          sources.append(selection);
          selection.select();
          copy.textContent = "Select and copy path above";
        } else copy.textContent = "Copied";
      };
      if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(node.source).then(() => { copy.textContent = "Copied"; }, fallback);
      } else fallback();
    }, "cf-copy");
    sources.append(copy);
    fragment.append(sources);
    if (/^[a-z0-9-]+\.html$/.test(node.docPage)) {
      const link = element("a", "cf-doc", "Read documentation: " + node.docPage);
      link.href = node.docPage;
      fragment.append(link);
    }
    fragment.append(relationshipSection("Callees / downstream", outgoing.get(node.id), "out"),
      relationshipSection("Callers / upstream", incoming.get(node.id), "in"));
    detailSection.replaceChildren(fragment);
  }
  function renderGuided() {
    if (!state.guided) {
      guidedPanel.hidden = true;
      guidedSteps.replaceChildren();
      guidedNav.replaceChildren();
      return;
    }
    const path = state.guided;
    guidedPanel.hidden = false;
    guidedTitle.textContent = path.title;
    guidedSummary.textContent = path.summary;
    const fragment = document.createDocumentFragment();
    path.nodes.forEach((step, index) => {
      const node = nodes.get(step.id);
      const row = element("div", "cf-guided-step" + (index === state.guidedIndex ? " cf-guided-current" : ""));
      const buttonEl = button(`${index + 1}. ${step.id}`, () => setGuidedIndex(index), "cf-guided-node");
      if (index === state.guidedIndex) buttonEl.setAttribute("aria-current", "step");
      row.append(buttonEl, element("span", "cf-guided-via", step.via || "relationship"), element("p", "", step.note));
      if (node) row.append(element("p", "cf-guided-source", node.source + " — " + node.purpose));
      fragment.append(row);
    });
    guidedSteps.replaceChildren(fragment);
    const prev = button("Previous", () => setGuidedIndex(Math.max(0, state.guidedIndex - 1)), "cf-guided-prev");
    const next = button("Next", () => setGuidedIndex(Math.min(path.nodes.length - 1, state.guidedIndex + 1)), "cf-guided-next");
    prev.disabled = state.guidedIndex === 0;
    next.disabled = state.guidedIndex === path.nodes.length - 1;
    guidedNav.replaceChildren(prev, next);
  }
  function setGuidedIndex(index) {
    if (!state.guided) return;
    const path = guidedPaths.find(candidate => candidate.id === state.guided.id) || state.guided;
    state.guidedIndex = Math.max(0, Math.min(path.nodes.length - 1, index));
    state.selected = path.nodes[state.guidedIndex].id;
    state.path = [];
    render();
  }
  function selectGuided(id) {
    if (id === "free") {
      clearGuided();
      render();
      return;
    }
    const path = guidedPaths.find(candidate => candidate.id === id);
    if (!path) return;
    state.guided = path;
    state.guidedIndex = 0;
    state.selected = path.nodes[0].id;
    state.path = [];
    state.depth = 1;
    depth.value = "1";
    render();
  }
  function render() { renderList(); renderMap(); renderDetail(); renderGuided(); }
  search.addEventListener("input", () => { state.query = search.value.trim().toLowerCase(); renderList(); renderMap(); });
  filter.addEventListener("change", () => { state.filter = filter.value; renderList(); renderMap(); });
  depth.addEventListener("change", () => { state.depth = Number(depth.value); state.path = []; renderMap(); });
  guided.addEventListener("change", () => selectGuided(guided.value));
  render();
}());
