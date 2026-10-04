"use strict";

// ── helpers ────────────────────────────────────────────────────────────────
const $ = (id) => document.getElementById(id);
const el = (tag, props = {}, children = []) => {
  const node = Object.assign(document.createElement(tag), props);
  for (const child of [].concat(children)) {
    if (child != null) node.append(child);
  }
  return node;
};

async function api(path, options = {}) {
  const res = await fetch(`/api${path}`, {
    headers: { "Content-Type": "application/json" },
    ...options,
  });
  const text = await res.text();
  const data = text ? JSON.parse(text) : null;
  if (!res.ok) throw new Error(data?.error || `HTTP ${res.status}`);
  return data;
}

function toast(message, kind = "") {
  const node = el("div", { className: `toast ${kind}`, textContent: message });
  $("toasts").append(node);
  setTimeout(() => {
    node.style.opacity = "0";
    node.style.transition = "opacity .3s";
    setTimeout(() => node.remove(), 300);
  }, kind === "err" ? 7000 : 4000);
}

function fmtDate(iso) {
  if (!iso) return "—";
  const d = new Date(iso);
  const pad = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function fmtDuration(from, to) {
  if (!from) return "—";
  const ms = (to ? new Date(to) : new Date()) - new Date(from);
  const s = Math.max(0, Math.round(ms / 1000));
  if (s < 60) return `${s}초`;
  if (s < 3600) return `${Math.floor(s / 60)}분 ${s % 60}초`;
  return `${Math.floor(s / 3600)}시간 ${Math.floor((s % 3600) / 60)}분`;
}

// ── tabs ───────────────────────────────────────────────────────────────────
const LOADERS = {
  dashboard: () => loadOverview(),
  categories: () => loadCategories(),
  posts: () => loadPosts(),
  jobs: () => loadJobHistory(),
};

function showTab(name) {
  if (!LOADERS[name]) name = "dashboard";
  for (const t of document.querySelectorAll(".tab")) {
    t.classList.toggle("active", t.dataset.tab === name);
  }
  for (const p of document.querySelectorAll(".panel")) {
    p.classList.toggle("active", p.id === `panel-${name}`);
  }
  LOADERS[name]();
}

function activeTab() {
  return document.querySelector(".tab.active").dataset.tab;
}

/// Switches tab via the hash, falling back to a direct render when the hash
/// is already correct (in which case no `hashchange` fires).
function navigate(name) {
  if (location.hash.slice(1) === name) showTab(name);
  else location.hash = name;
}

$("tabs").addEventListener("click", (e) => {
  const tab = e.target.closest(".tab");
  if (tab) navigate(tab.dataset.tab);
});

// Tabs are addressable via the URL hash, so a reload keeps you where you were.
addEventListener("hashchange", () => showTab(location.hash.slice(1)));

// ── job control ────────────────────────────────────────────────────────────
let logCursor = 0;
let lastJobId = null;
let jobWasRunning = false;

document.addEventListener("click", (e) => {
  const btn = e.target.closest("[data-job]");
  if (btn) startJob(btn.dataset.job);
});

async function startJob(kind) {
  try {
    const job = await api(`/jobs/${kind}`, { method: "POST" });
    toast(`'${job.label}' 작업을 시작했습니다.`);
    resetLog(job.id);
    goToDashboardLog();
    pollJob();
  } catch (e) {
    toast(e.message, "err");
  }
}

function resetLog(jobId) {
  lastJobId = jobId;
  logCursor = 0;
  $("live-log").textContent = "";
}

function goToDashboardLog() {
  navigate("dashboard");
  $("live-log").scrollIntoView({ behavior: "smooth", block: "center" });
}

function setJobButtonsDisabled(disabled) {
  for (const b of document.querySelectorAll("[data-job], #post-resync, #post-resync-refetch, #cat-resync")) {
    b.disabled = disabled;
  }
}

async function pollJob() {
  let status;
  try {
    status = await api(`/jobs/status?from=${logCursor}`);
  } catch {
    return; // transient; the interval will retry
  }

  const job = status.job;
  const indicator = $("job-indicator");
  const badge = $("current-job-badge");

  if (!job) {
    indicator.hidden = true;
    setJobButtonsDisabled(false);
    return;
  }

  // A different job started (e.g. from another browser tab): restart the log.
  if (lastJobId !== null && job.id !== lastJobId) {
    resetLog(job.id);
    status = await api(`/jobs/status?from=0`);
  }
  lastJobId = job.id;

  if (job.logs.length) {
    const log = $("live-log");
    const atBottom = log.scrollHeight - log.scrollTop - log.clientHeight < 40;
    log.textContent += (log.textContent ? "\n" : "") + job.logs.join("\n");
    logCursor = status.log_len;
    if (atBottom) log.scrollTop = log.scrollHeight;
  }

  const running = job.status === "running";
  indicator.hidden = !running;
  $("job-indicator-text").textContent = `${job.label} 실행 중…`;
  setJobButtonsDisabled(running);

  badge.textContent = { running: "실행 중", success: "완료", failed: "실패" }[job.status];
  badge.className = `badge ${{ running: "run", success: "ok", failed: "err" }[job.status]}`;

  $("current-job-meta").textContent =
    `${job.label} · 시작 ${fmtDate(job.started_at)} · ${fmtDuration(job.started_at, job.finished_at)}` +
    (job.message ? ` · ${job.message}` : "");

  // On completion, refresh whatever the user is looking at.
  if (jobWasRunning && !running) {
    toast(
      `'${job.label}' ${job.status === "success" ? "완료" : "실패"}: ${job.message ?? ""}`,
      job.status === "success" ? "ok" : "err",
    );
    const active = activeTab();
    LOADERS[active]();
    if (active !== "dashboard") loadOverview();
  }
  jobWasRunning = running;
}

// ── dashboard ──────────────────────────────────────────────────────────────
async function loadOverview() {
  let data;
  try {
    data = await api("/overview");
  } catch (e) {
    toast(e.message, "err");
    return;
  }

  $("blog-id").textContent = `네이버 블로그: ${data.blog_id}`;

  const p = data.posts;
  const pending = p.total - p.replicated;
  const stats = [
    ["수집된 글", p.total, ""],
    ["본문 수집됨", p.fetched, p.fetched < p.total ? "warn" : ""],
    ["복제 완료", p.replicated, "ok"],
    ["미복제", pending, pending > 0 ? "warn" : ""],
    ["복제 오류", p.errors, p.errors > 0 ? "err" : ""],
    ["복제 대상 카테고리", `${data.categories.mirrored} / ${data.categories.total}`, ""],
  ];

  $("stat-grid").replaceChildren(
    ...stats.map(([label, value, kind]) =>
      el("div", { className: "stat" }, [
        el("div", { className: "label", textContent: label }),
        el("div", { className: `value ${kind}`, textContent: String(value) }),
      ]),
    ),
  );

  const info = [
    ["동기화 커서 (last logNo)", String(data.cursor)],
    ["최신 글 작성일", fmtDate(p.latest_post_at)],
    ["마지막 복제 시각", fmtDate(p.last_replicated_at)],
    ["로컬 저장소 경로", data.repo_path],
    ["GitHub 원격", data.remote_url],
  ];
  $("repo-info").replaceChildren(
    ...info.flatMap(([k, v]) => [
      el("dt", { textContent: k }),
      el("dd", { textContent: v }),
    ]),
  );
}

// ── categories ─────────────────────────────────────────────────────────────
let categories = [];

async function loadCategories() {
  try {
    categories = await api("/categories");
  } catch (e) {
    toast(e.message, "err");
    return;
  }

  const byNo = new Map(categories.map((c) => [c.category_no, c]));
  const body = $("cat-body");

  if (!categories.length) {
    body.replaceChildren(
      el("tr", {}, el("td", {
        colSpan: 8,
        className: "empty",
        textContent: "카테고리가 없습니다. '네이버에서 다시 가져오기'를 눌러주세요.",
      })),
    );
    syncCatSelection();
    return;
  }

  body.replaceChildren(...categories.map((c) => {
    const parent = c.parent_no != null ? byNo.get(c.parent_no) : null;
    const name = parent ? `${parent.name} › ${c.name}` : c.name;

    const check = el("input", { type: "checkbox", className: "cat-check", value: c.category_no });
    check.addEventListener("change", syncCatSelection);

    const nameInput = el("input", {
      type: "text",
      value: c.display_name ?? "",
      placeholder: c.name,
      style: "width:100%",
    });
    nameInput.addEventListener("change", () =>
      saveCategory(c.category_no, { display_name: nameInput.value.trim() || null }),
    );

    const toggle = el("input", { type: "checkbox", checked: c.should_mirror });
    toggle.addEventListener("change", () =>
      saveCategory(c.category_no, { should_mirror: toggle.checked }),
    );

    return el("tr", {}, [
      el("td", { className: "col-check" }, check),
      el("td", { className: "col-no", textContent: String(c.category_no) }),
      el("td", { textContent: name }),
      el("td", {}, nameInput),
      el("td", { className: "col-num", textContent: String(c.post_count) }),
      el("td", { className: "col-num", textContent: String(c.db_post_count) }),
      el("td", { className: "col-num", textContent: String(c.replicated_count) }),
      el("td", { className: "col-mirror" },
        el("label", { className: "switch" }, [toggle, el("span", { className: "track" })])),
    ]);
  }));

  $("cat-check-all").checked = false;
  syncCatSelection();
  fillCategoryFilter();
}

async function saveCategory(categoryNo, patch) {
  try {
    await api(`/categories/${categoryNo}`, { method: "PATCH", body: JSON.stringify(patch) });
    toast("저장했습니다.", "ok");
    loadOverview();
  } catch (e) {
    toast(e.message, "err");
    loadCategories();
  }
}

function selectedCategories() {
  return [...document.querySelectorAll(".cat-check:checked")].map((c) => Number(c.value));
}

function syncCatSelection() {
  const n = selectedCategories().length;
  $("cat-bulk").hidden = n === 0;
  $("cat-selected-count").textContent = `${n}개 선택됨`;
}

$("cat-check-all").addEventListener("change", (e) => {
  for (const c of document.querySelectorAll(".cat-check")) c.checked = e.target.checked;
  syncCatSelection();
});

for (const [id, value] of [["cat-mirror-on", true], ["cat-mirror-off", false]]) {
  $(id).addEventListener("click", async () => {
    try {
      await api("/categories/mirror", {
        method: "POST",
        body: JSON.stringify({ category_nos: selectedCategories(), should_mirror: value }),
      });
      toast(`복제를 ${value ? "켰" : "껐"}습니다.`, "ok");
      loadCategories();
      loadOverview();
    } catch (e) {
      toast(e.message, "err");
    }
  });
}

$("cat-resync").addEventListener("click", () => {
  const nos = selectedCategories();
  const count = categories
    .filter((c) => nos.includes(c.category_no))
    .reduce((sum, c) => sum + c.db_post_count, 0);
  if (!confirm(`선택한 ${nos.length}개 카테고리의 글 ${count}건을 모두 재발행합니다. 계속할까요?`)) return;
  startResync({ category_nos: nos, refetch: false });
});

// ── posts ──────────────────────────────────────────────────────────────────
let postPage = 1;

function fillCategoryFilter() {
  const select = $("post-category");
  const current = select.value;
  select.replaceChildren(
    el("option", { value: "", textContent: "모든 카테고리" }),
    ...categories.map((c) =>
      el("option", {
        value: String(c.category_no),
        textContent: `${c.name} (${c.db_post_count})`,
      }),
    ),
  );
  select.value = current;
}

async function loadPosts() {
  // The category filter is cosmetic, so populate it in the background rather
  // than making the post list wait on a second round-trip.
  if (!categories.length) {
    api("/categories")
      .then((c) => { categories = c; fillCategoryFilter(); })
      .catch(() => { /* filter just stays empty */ });
  }

  const params = new URLSearchParams({ page: postPage, size: 30, status: $("post-status").value });
  if ($("post-q").value.trim()) params.set("q", $("post-q").value.trim());
  if ($("post-category").value) params.set("category_no", $("post-category").value);

  let data;
  try {
    data = await api(`/posts?${params}`);
  } catch (e) {
    toast(e.message, "err");
    return;
  }

  $("post-total").textContent = `${data.total}건`;
  const body = $("post-body");

  if (!data.items.length) {
    body.replaceChildren(
      el("tr", {}, el("td", { colSpan: 7, className: "empty", textContent: "조건에 맞는 게시글이 없습니다." })),
    );
  } else {
    body.replaceChildren(...data.items.map(renderPostRow));
  }

  renderPager(data);
  $("post-check-all").checked = false;
  syncPostSelection();
}

function renderPostRow(p) {
  const check = el("input", { type: "checkbox", className: "post-check", value: p.log_no });
  check.addEventListener("change", syncPostSelection);

  const categoryLabel = p.category_no == null
    ? "—"
    : (p.category_display_name || p.category_name || `#${p.category_no}`);

  const resyncBtn = el("button", {
    className: "btn small",
    textContent: "재동기화",
    title: "네이버에서 본문을 다시 받아 재발행합니다",
  });
  resyncBtn.addEventListener("click", () => {
    if (!confirm(`'${p.title}'을(를) 네이버에서 다시 받아 재발행할까요?`)) return;
    startResync({ log_nos: [p.log_no], refetch: true });
  });

  const viewBtn = el("button", { className: "btn link", textContent: "본문" });
  viewBtn.addEventListener("click", () => showPostBody(p.log_no));

  return el("tr", {}, [
    el("td", { className: "col-check" }, check),
    el("td", { className: "col-no", textContent: String(p.log_no) }),
    el("td", { className: "title", title: p.title }, p.title),
    el("td", {}, [
      document.createTextNode(categoryLabel),
      p.category_no != null && !p.category_mirrored
        ? el("span", { className: "badge", textContent: "복제 off", style: "margin-left:6px" })
        : null,
    ]),
    el("td", { className: "col-date", textContent: fmtDate(p.add_date) }),
    el("td", { className: "col-status" }, postStatusBadge(p)),
    el("td", { className: "col-actions" }, [viewBtn, resyncBtn]),
  ]);
}

function postStatusBadge(p) {
  if (p.replication_error) {
    return el("span", { className: "badge err", textContent: "오류", title: p.replication_error });
  }
  if (p.replicated_at) {
    return el("span", { className: "badge ok", textContent: "복제됨", title: fmtDate(p.replicated_at) });
  }
  if (!p.has_body) return el("span", { className: "badge warn", textContent: "본문 없음" });
  return el("span", { className: "badge", textContent: "대기" });
}

function renderPager({ total, page, size }) {
  const pages = Math.max(1, Math.ceil(total / size));
  const mk = (label, target, disabled) => {
    const b = el("button", { className: "btn small", textContent: label, disabled });
    b.addEventListener("click", () => { postPage = target; loadPosts(); });
    return b;
  };
  $("post-pager").replaceChildren(
    mk("« 처음", 1, page <= 1),
    mk("‹ 이전", page - 1, page <= 1),
    el("span", { className: "page-info", textContent: `${page} / ${pages}` }),
    mk("다음 ›", page + 1, page >= pages),
    mk("마지막 »", pages, page >= pages),
  );
}

function selectedPosts() {
  return [...document.querySelectorAll(".post-check:checked")].map((c) => Number(c.value));
}

function syncPostSelection() {
  const n = selectedPosts().length;
  $("post-bulk").hidden = n === 0;
  $("post-selected-count").textContent = `${n}건 선택됨`;
}

$("post-check-all").addEventListener("change", (e) => {
  for (const c of document.querySelectorAll(".post-check")) c.checked = e.target.checked;
  syncPostSelection();
});

$("post-search").addEventListener("click", () => { postPage = 1; loadPosts(); });
$("post-q").addEventListener("keydown", (e) => {
  if (e.key === "Enter") { postPage = 1; loadPosts(); }
});
for (const id of ["post-category", "post-status"]) {
  $(id).addEventListener("change", () => { postPage = 1; loadPosts(); });
}

$("post-resync-refetch").addEventListener("click", () =>
  confirmResync(selectedPosts(), true));
$("post-resync").addEventListener("click", () =>
  confirmResync(selectedPosts(), false));

function confirmResync(logNos, refetch) {
  const what = refetch ? "네이버에서 본문을 다시 받아 재발행" : "저장된 본문으로 재발행";
  if (!confirm(`선택한 ${logNos.length}건을 ${what}합니다. 계속할까요?`)) return;
  startResync({ log_nos: logNos, refetch });
}

async function startResync(payload) {
  try {
    const job = await api("/jobs/resync", { method: "POST", body: JSON.stringify(payload) });
    toast("재동기화를 시작했습니다.");
    resetLog(job.id);
    goToDashboardLog();
    pollJob();
  } catch (e) {
    toast(e.message, "err");
  }
}

async function showPostBody(logNo) {
  try {
    const data = await api(`/posts/${logNo}/body`);
    openModal(
      `${data.title}`,
      data.markdown ?? "(저장된 본문이 없습니다. 재동기화로 수집할 수 있습니다.)",
    );
  } catch (e) {
    toast(e.message, "err");
  }
}

// ── job history ────────────────────────────────────────────────────────────
async function loadJobHistory() {
  let rows;
  try {
    rows = await api("/jobs/history");
  } catch (e) {
    toast(e.message, "err");
    return;
  }

  const body = $("jobs-body");
  if (!rows.length) {
    body.replaceChildren(
      el("tr", {}, el("td", { colSpan: 7, className: "empty", textContent: "실행 이력이 없습니다." })),
    );
    return;
  }

  const badgeKind = { success: "ok", failed: "err", running: "run", interrupted: "warn" };
  const statusLabel = { success: "성공", failed: "실패", running: "실행 중", interrupted: "중단됨" };

  body.replaceChildren(...rows.map((r) => {
    const logBtn = el("button", { className: "btn small", textContent: "로그" });
    logBtn.addEventListener("click", async () => {
      try {
        const data = await api(`/jobs/history/${r.id}/log`);
        openModal(`#${r.id} ${r.kind}`, data.log || "(로그 없음)");
      } catch (e) {
        toast(e.message, "err");
      }
    });

    return el("tr", {}, [
      el("td", { className: "col-no", textContent: String(r.id) }),
      el("td", { textContent: r.kind }),
      el("td", { className: "col-status" },
        el("span", {
          className: `badge ${badgeKind[r.status] ?? ""}`,
          textContent: statusLabel[r.status] ?? r.status,
        })),
      el("td", { textContent: r.message ?? "—" }),
      el("td", { className: "col-date", textContent: fmtDate(r.started_at) }),
      el("td", { className: "col-date", textContent: r.finished_at ? fmtDuration(r.started_at, r.finished_at) : "—" }),
      el("td", { className: "col-actions" }, logBtn),
    ]);
  }));
}

$("jobs-refresh").addEventListener("click", loadJobHistory);

// ── modal ──────────────────────────────────────────────────────────────────
function openModal(title, body) {
  $("modal-title").textContent = title;
  $("modal-body").textContent = body;
  $("modal").hidden = false;
}
$("modal-close").addEventListener("click", () => { $("modal").hidden = true; });
$("modal").addEventListener("click", (e) => {
  if (e.target === $("modal")) $("modal").hidden = true;
});
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") $("modal").hidden = true;
});

// ── boot ───────────────────────────────────────────────────────────────────
showTab(location.hash.slice(1));
// Header info comes from the overview, which only the dashboard tab loads.
if (activeTab() !== "dashboard") loadOverview();
pollJob();
setInterval(pollJob, 2000);
