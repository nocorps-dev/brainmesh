const state = {
  view: "overview",
  metrics: null,
  knowledge: [],
  agents: [],
  federation: [],
  audit: [],
  searchResults: [],
};

const apiUrlEl = document.getElementById("apiUrl");
const apiKeyEl = document.getElementById("apiKey");
const statusPill = document.getElementById("statusPill");
const viewTitle = document.getElementById("viewTitle");
const viewContent = document.getElementById("viewContent");

async function apiFetch(path, options = {}) {
  const base = apiUrlEl.value.replace(/\/+$/, "");
  const headers = { "Content-Type": "application/json", ...(options.headers || {}) };
  if (apiKeyEl.value.trim()) headers.Authorization = `Bearer ${apiKeyEl.value.trim()}`;
  const res = await fetch(`${base}${path}`, { ...options, headers });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

async function refreshAll() {
  try {
    state.metrics = await apiFetch("/api/v1/metrics");
    statusPill.textContent = `Connected (${state.metrics.version})`;
    statusPill.className = "pill ok";
    state.knowledge = await apiFetch("/api/v1/knowledge?limit=50").catch(() => []);
    state.agents = await apiFetch("/api/v1/agents").catch(() => []);
    state.federation = await apiFetch("/api/v1/federation/nodes").catch(() => []);
    state.audit = await apiFetch("/api/v1/audit").catch(() => []);
  } catch (err) {
    statusPill.textContent = `Offline (${err.message})`;
    statusPill.className = "pill err";
  }
  render();
}

function escapeHtml(str) {
  return String(str ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

function render() {
  if (state.view === "overview") {
    const m = state.metrics || { total_agents: 0, total_knowledge: 0, verified_knowledge: 0, total_verifications: 0 };
    viewTitle.textContent = "Overview";
    viewContent.innerHTML = `
      <div class="metrics-row">
        <div class="stat"><div class="label">Registered Agents</div><div class="value">${m.total_agents}</div></div>
        <div class="stat"><div class="label">Knowledge Objects</div><div class="value">${m.total_knowledge}</div></div>
        <div class="stat"><div class="label">Verified Knowledge</div><div class="value">${m.verified_knowledge}</div></div>
        <div class="stat"><div class="label">Verification Events</div><div class="value">${m.total_verifications}</div></div>
      </div>`;
  } else if (state.view === "knowledge") {
    viewTitle.textContent = "Knowledge Explorer";
    viewContent.innerHTML = `
      <table>
        <thead><tr><th>Title</th><th>Domain</th><th>Visibility</th><th>Verification</th><th>Framework</th></tr></thead>
        <tbody>
          ${state.knowledge.map((k) => `
            <tr>
              <td>${escapeHtml(k.title)}</td>
              <td>${escapeHtml(k.domain)}</td>
              <td>${escapeHtml(k.visibility)}</td>
              <td>${escapeHtml(k.verification?.status)}</td>
              <td>${escapeHtml(k.provenance?.framework)}</td>
            </tr>`).join("")}
        </tbody>
      </table>`;
  } else if (state.view === "search") {
    viewTitle.textContent = "Search Playground";
    viewContent.innerHTML = `
      <div class="search-bar">
        <input id="recallInput" type="text" placeholder="Search collective intelligence (e.g. PostgreSQL connection failure)..." />
        <button id="recallBtn" class="btn-primary">Recall</button>
      </div>
      <div id="recallResults">
        ${state.searchResults.map((r) => `
          <div class="stat" style="margin-bottom:10px">
            <div><strong>${escapeHtml(r.knowledge.title)}</strong> — ${escapeHtml(r.knowledge.verification.status)} (score: ${(r.score?.total_score ?? 0).toFixed(2)})</div>
            <div class="label" style="margin-top:4px">${escapeHtml(r.knowledge.description)}</div>
            <div class="notice" style="margin-top:6px">${escapeHtml(r.safety_notice)}</div>
          </div>`).join("")}
      </div>`;
    document.getElementById("recallBtn").onclick = async () => {
      const q = document.getElementById("recallInput").value;
      state.searchResults = await apiFetch("/api/v1/recall", { method: "POST", body: JSON.stringify({ query: q, limit: 10 }) });
      render();
    };
  } else if (state.view === "agents") {
    viewTitle.textContent = "Agents";
    viewContent.innerHTML = `
      <table>
        <thead><tr><th>Name</th><th>Framework</th><th>Model</th><th>Created</th></tr></thead>
        <tbody>${state.agents.map((a) => `<tr><td>${escapeHtml(a.name)}</td><td>${escapeHtml(a.framework)}</td><td>${escapeHtml(a.model || "-")}</td><td>${escapeHtml(a.created_at)}</td></tr>`).join("")}</tbody>
      </table>`;
  } else if (state.view === "federation") {
    viewTitle.textContent = "Federation";
    viewContent.innerHTML = `
      <table>
        <thead><tr><th>Node</th><th>URL</th><th>Trust Tier</th><th>Last Sync</th></tr></thead>
        <tbody>${state.federation.map((n) => `<tr><td>${escapeHtml(n.name)}</td><td>${escapeHtml(n.url)}</td><td>${escapeHtml(n.trust_tier)}</td><td>${escapeHtml(n.last_sync_at || "Never")}</td></tr>`).join("")}</tbody>
      </table>`;
  } else if (state.view === "audit") {
    viewTitle.textContent = "Audit Logs";
    viewContent.innerHTML = `
      <table>
        <thead><tr><th>Principal</th><th>Action</th><th>Resource</th><th>Allowed</th><th>Timestamp</th></tr></thead>
        <tbody>${state.audit.map((l) => `<tr><td>${escapeHtml(l.principal)}</td><td>${escapeHtml(l.action)}</td><td>${escapeHtml(l.resource_type)}:${escapeHtml(l.resource_id)}</td><td>${l.allowed}</td><td>${escapeHtml(l.created_at)}</td></tr>`).join("")}</tbody>
      </table>`;
  }
}

document.querySelectorAll(".nav-item").forEach((btn) => {
  btn.onclick = () => {
    document.querySelectorAll(".nav-item").forEach((b) => b.classList.remove("active"));
    btn.classList.add("active");
    state.view = btn.dataset.view;
    render();
  };
});

document.getElementById("refreshBtn").onclick = refreshAll;
refreshAll();
