// ── Crons page ───────────────────────────────────────────

import { sendRpc } from "./helpers.js";
import { closeProviderModal } from "./providers.js";
import { registerPage } from "./router.js";
import * as S from "./state.js";

var providerModal = S.$("providerModal");
var providerModalTitle = S.$("providerModalTitle");
var providerModalBody = S.$("providerModalBody");

// Safe: static hardcoded HTML template string — no user input is interpolated.
var cronsPageHTML =
	'<div class="flex-1 flex flex-col min-w-0 p-4 gap-4 overflow-y-auto">' +
	'<div class="flex items-center gap-3">' +
	'<h2 class="text-lg font-medium text-[var(--text-strong)]">Cron Jobs</h2>' +
	'<button id="cronAddBtn" class="bg-[var(--accent-dim)] text-white border-none px-3 py-1.5 rounded text-xs cursor-pointer hover:bg-[var(--accent)] transition-colors">+ Add Job</button>' +
	'<button id="cronRefreshBtn" class="text-xs text-[var(--muted)] border border-[var(--border)] px-2.5 py-1 rounded-md hover:text-[var(--text)] hover:border-[var(--border-strong)] transition-colors cursor-pointer bg-transparent">Refresh</button>' +
	"</div>" +
	'<div id="cronStatusBar" class="cron-status-bar"></div>' +
	'<div id="cronJobList"></div>' +
	'<div id="cronRunsPanel" class="hidden"></div>' +
	"</div>";

registerPage("/crons", function initCrons(container) {
	container.innerHTML = cronsPageHTML; // safe: static template, no user input

	var cronStatusBar = S.$("cronStatusBar");
	var cronJobList = S.$("cronJobList");
	var cronRunsPanel = S.$("cronRunsPanel");

	function loadStatus() {
		sendRpc("cron.status", {}).then((res) => {
			if (!res || !res.ok) {
				cronStatusBar.textContent = "Failed to load status";
				return;
			}
			var s = res.payload;
			var parts = [
				s.running ? "Running" : "Stopped",
				`${s.jobCount} job${s.jobCount !== 1 ? "s" : ""}`,
				`${s.enabledCount} enabled`,
			];
			if (s.nextRunAtMs) {
				parts.push(`next: ${new Date(s.nextRunAtMs).toLocaleString()}`);
			}
			cronStatusBar.textContent = parts.join(" \u2022 ");
		});
	}

	function loadJobs() {
		sendRpc("cron.list", {}).then((res) => {
			if (!res || !res.ok) {
				cronJobList.textContent = "Failed to load jobs";
				return;
			}
			renderJobTable(res.payload || []);
		});
	}

	function renderJobTable(jobs) {
		cronJobList.textContent = "";
		if (jobs.length === 0) {
			var empty = document.createElement("div");
			empty.className = "text-sm text-[var(--muted)]";
			empty.textContent = "No cron jobs configured.";
			cronJobList.appendChild(empty);
			return;
		}
		var table = document.createElement("table");
		table.className = "cron-table";

		var thead = document.createElement("thead");
		var headRow = document.createElement("tr");
		[
			"Name",
			"Schedule",
			"Enabled",
			"Next Run",
			"Last Status",
			"Actions",
		].forEach((h) => {
			var th = document.createElement("th");
			th.textContent = h;
			headRow.appendChild(th);
		});
		thead.appendChild(headRow);
		table.appendChild(thead);

		var tbody = document.createElement("tbody");
		jobs.forEach((job) => {
			var tr = document.createElement("tr");

			var tdName = document.createElement("td");
			tdName.textContent = job.name;
			tr.appendChild(tdName);

			var tdSched = document.createElement("td");
			tdSched.textContent = formatSchedule(job.schedule);
			tdSched.className = "cron-mono";
			tr.appendChild(tdSched);

			var tdEnabled = document.createElement("td");
			var toggle = document.createElement("label");
			toggle.className = "cron-toggle";
			var checkbox = document.createElement("input");
			checkbox.type = "checkbox";
			checkbox.checked = job.enabled;
			checkbox.addEventListener("change", () => {
				sendRpc("cron.update", {
					id: job.id,
					patch: { enabled: checkbox.checked },
				}).then(() => {
					loadStatus();
				});
			});
			toggle.appendChild(checkbox);
			var slider = document.createElement("span");
			slider.className = "cron-slider";
			toggle.appendChild(slider);
			tdEnabled.appendChild(toggle);
			tr.appendChild(tdEnabled);

			var tdNext = document.createElement("td");
			tdNext.className = "cron-mono";
			tdNext.textContent = job.state?.nextRunAtMs
				? new Date(job.state.nextRunAtMs).toLocaleString()
				: "\u2014";
			tr.appendChild(tdNext);

			var tdStatus = document.createElement("td");
			if (job.state?.lastStatus) {
				var badge = document.createElement("span");
				badge.className = `cron-badge ${job.state.lastStatus}`;
				badge.textContent = job.state.lastStatus;
				tdStatus.appendChild(badge);
			} else {
				tdStatus.textContent = "\u2014";
			}
			tr.appendChild(tdStatus);

			var tdActions = document.createElement("td");
			tdActions.className = "cron-actions";

			var editBtn = document.createElement("button");
			editBtn.className = "cron-action-btn";
			editBtn.textContent = "Edit";
			editBtn.addEventListener("click", () => {
				openCronModal(job);
			});
			tdActions.appendChild(editBtn);

			var runBtn = document.createElement("button");
			runBtn.className = "cron-action-btn";
			runBtn.textContent = "Run";
			runBtn.addEventListener("click", () => {
				sendRpc("cron.run", { id: job.id, force: true }).then(() => {
					loadJobs();
					loadStatus();
				});
			});
			tdActions.appendChild(runBtn);

			var histBtn = document.createElement("button");
			histBtn.className = "cron-action-btn";
			histBtn.textContent = "History";
			histBtn.addEventListener("click", () => {
				showRunHistory(job.id, job.name);
			});
			tdActions.appendChild(histBtn);

			var delBtn = document.createElement("button");
			delBtn.className = "cron-action-btn cron-action-danger";
			delBtn.textContent = "Delete";
			delBtn.addEventListener("click", () => {
				if (confirm(`Delete job '${job.name}'?`)) {
					sendRpc("cron.remove", { id: job.id }).then(() => {
						loadJobs();
						loadStatus();
					});
				}
			});
			tdActions.appendChild(delBtn);

			tr.appendChild(tdActions);
			tbody.appendChild(tr);
		});
		table.appendChild(tbody);
		cronJobList.appendChild(table);
	}

	function formatSchedule(sched) {
		if (sched.kind === "at")
			return `At ${new Date(sched.atMs).toLocaleString()}`;
		if (sched.kind === "every") {
			var ms = sched.everyMs;
			if (ms >= 3600000) return `Every ${ms / 3600000}h`;
			if (ms >= 60000) return `Every ${ms / 60000}m`;
			return `Every ${ms / 1000}s`;
		}
		if (sched.kind === "cron")
			return sched.expr + (sched.tz ? ` (${sched.tz})` : "");
		return JSON.stringify(sched);
	}

	function showRunHistory(jobId, jobName) {
		cronRunsPanel.classList.remove("hidden");
		cronRunsPanel.textContent = "";
		var loading = document.createElement("div");
		loading.className = "text-sm text-[var(--muted)]";
		loading.textContent = `Loading history for ${jobName}...`;
		cronRunsPanel.appendChild(loading);

		sendRpc("cron.runs", { id: jobId }).then((res) => {
			cronRunsPanel.textContent = "";
			if (!res || !res.ok) {
				var errEl = document.createElement("div");
				errEl.className = "text-sm text-[var(--error)]";
				errEl.textContent = "Failed to load history";
				cronRunsPanel.appendChild(errEl);
				return;
			}
			var runs = res.payload || [];

			var header = document.createElement("div");
			header.className = "flex items-center justify-between";
			header.classList.add("mb-md");
			var titleEl = document.createElement("span");
			titleEl.className = "text-sm font-medium text-[var(--text-strong)]";
			titleEl.textContent = `Run History: ${jobName}`;
			header.appendChild(titleEl);
			var closeBtn = document.createElement("button");
			closeBtn.className =
				"text-xs text-[var(--muted)] cursor-pointer bg-transparent border-none hover:text-[var(--text)]";
			closeBtn.textContent = "\u2715 Close";
			closeBtn.addEventListener("click", () => {
				cronRunsPanel.classList.add("hidden");
			});
			header.appendChild(closeBtn);
			cronRunsPanel.appendChild(header);

			if (runs.length === 0) {
				var emptyEl = document.createElement("div");
				emptyEl.className = "text-xs text-[var(--muted)]";
				emptyEl.textContent = "No runs yet.";
				cronRunsPanel.appendChild(emptyEl);
				return;
			}

			runs.forEach((run) => {
				var item = document.createElement("div");
				item.className = "cron-run-item";
				var time = document.createElement("span");
				time.className = "text-xs text-[var(--muted)]";
				time.textContent = new Date(run.startedAtMs).toLocaleString();
				item.appendChild(time);
				var runBadge = document.createElement("span");
				runBadge.className = `cron-badge ${run.status}`;
				runBadge.textContent = run.status;
				item.appendChild(runBadge);
				var dur = document.createElement("span");
				dur.className = "text-xs text-[var(--muted)]";
				dur.textContent = `${run.durationMs}ms`;
				item.appendChild(dur);
				if (run.error) {
					var errSpan = document.createElement("span");
					errSpan.className = "text-xs text-[var(--error)]";
					errSpan.textContent = run.error;
					item.appendChild(errSpan);
				}
				cronRunsPanel.appendChild(item);
			});
		});
	}

	function openCronModal(existingJob) {
		var isEdit = !!existingJob;
		providerModal.classList.remove("hidden");
		providerModalTitle.textContent = isEdit ? "Edit Job" : "Add Job";
		providerModalBody.textContent = "";

		var form = document.createElement("div");
		form.className = "provider-key-form";

		function addField(labelText, el) {
			var lbl = document.createElement("label");
			lbl.className = "text-xs text-[var(--muted)]";
			lbl.textContent = labelText;
			form.appendChild(lbl);
			form.appendChild(el);
		}

		var nameInput = document.createElement("input");
		nameInput.className = "provider-key-input";
		nameInput.placeholder = "Job name";
		nameInput.value = isEdit ? existingJob.name : "";
		addField("Name", nameInput);

		var schedSelect = document.createElement("select");
		schedSelect.className = "provider-key-input";
		["at", "every", "cron"].forEach((k) => {
			var opt = document.createElement("option");
			opt.value = k;
			opt.textContent =
				k === "at"
					? "At (one-shot)"
					: k === "every"
						? "Every (interval)"
						: "Cron (expression)";
			schedSelect.appendChild(opt);
		});
		addField("Schedule Type", schedSelect);

		var schedParams = document.createElement("div");
		form.appendChild(schedParams);

		var schedAtInput = document.createElement("input");
		schedAtInput.className = "provider-key-input";
		schedAtInput.type = "datetime-local";
		var schedEveryInput = document.createElement("input");
		schedEveryInput.className = "provider-key-input";
		schedEveryInput.type = "number";
		schedEveryInput.placeholder = "Interval in seconds";
		schedEveryInput.min = "1";
		var schedCronInput = document.createElement("input");
		schedCronInput.className = "provider-key-input";
		schedCronInput.placeholder = "*/5 * * * *";
		var schedTzInput = document.createElement("input");
		schedTzInput.className = "provider-key-input";
		schedTzInput.placeholder = "Timezone (optional, e.g. Europe/Paris)";

		function updateSchedParams() {
			schedParams.textContent = "";
			var kind = schedSelect.value;
			if (kind === "at") schedParams.appendChild(schedAtInput);
			else if (kind === "every") schedParams.appendChild(schedEveryInput);
			else {
				schedParams.appendChild(schedCronInput);
				schedParams.appendChild(schedTzInput);
			}
		}
		schedSelect.addEventListener("change", updateSchedParams);

		var payloadSelect = document.createElement("select");
		payloadSelect.className = "provider-key-input";
		["systemEvent", "agentTurn"].forEach((k) => {
			var opt = document.createElement("option");
			opt.value = k;
			opt.textContent = k === "systemEvent" ? "System Event" : "Agent Turn";
			payloadSelect.appendChild(opt);
		});
		addField("Payload Type", payloadSelect);

		var payloadTextInput = document.createElement("textarea");
		payloadTextInput.className = "provider-key-input";
		payloadTextInput.placeholder = "Message text";
		payloadTextInput.classList.add("textarea-sm");
		addField("Message", payloadTextInput);

		var targetSelect = document.createElement("select");
		targetSelect.className = "provider-key-input";
		["isolated", "main"].forEach((k) => {
			var opt = document.createElement("option");
			opt.value = k;
			opt.textContent = k.charAt(0).toUpperCase() + k.slice(1);
			targetSelect.appendChild(opt);
		});
		addField("Session Target", targetSelect);

		var deleteAfterLabel = document.createElement("label");
		deleteAfterLabel.className =
			"text-xs text-[var(--muted)] flex items-center gap-2";
		var deleteAfterCheck = document.createElement("input");
		deleteAfterCheck.type = "checkbox";
		deleteAfterLabel.appendChild(deleteAfterCheck);
		deleteAfterLabel.appendChild(document.createTextNode("Delete after run"));
		form.appendChild(deleteAfterLabel);

		var enabledLabel = document.createElement("label");
		enabledLabel.className =
			"text-xs text-[var(--muted)] flex items-center gap-2";
		var enabledCheck = document.createElement("input");
		enabledCheck.type = "checkbox";
		enabledCheck.checked = true;
		enabledLabel.appendChild(enabledCheck);
		enabledLabel.appendChild(document.createTextNode("Enabled"));
		form.appendChild(enabledLabel);

		if (isEdit) {
			var sc = existingJob.schedule;
			schedSelect.value = sc.kind;
			if (sc.kind === "at" && sc.atMs)
				schedAtInput.value = new Date(sc.atMs).toISOString().slice(0, 16);
			else if (sc.kind === "every" && sc.everyMs)
				schedEveryInput.value = Math.round(sc.everyMs / 1000);
			else if (sc.kind === "cron") {
				schedCronInput.value = sc.expr || "";
				schedTzInput.value = sc.tz || "";
			}
			var p = existingJob.payload;
			payloadSelect.value = p.kind;
			payloadTextInput.value = p.text || p.message || "";
			targetSelect.value = existingJob.sessionTarget || "isolated";
			deleteAfterCheck.checked = existingJob.deleteAfterRun || false;
			enabledCheck.checked = existingJob.enabled;
		}
		updateSchedParams();

		var btns = document.createElement("div");
		btns.className = "btn-row-mt";

		var cancelBtn = document.createElement("button");
		cancelBtn.className = "provider-btn provider-btn-secondary";
		cancelBtn.textContent = "Cancel";
		cancelBtn.addEventListener("click", closeProviderModal);
		btns.appendChild(cancelBtn);

		var saveBtn = document.createElement("button");
		saveBtn.className = "provider-btn";
		saveBtn.textContent = isEdit ? "Update" : "Create";
		saveBtn.addEventListener("click", () => {
			var name = nameInput.value.trim();
			if (!name) {
				nameInput.classList.add("field-error");
				return;
			}
			var schedule,
				kind = schedSelect.value;
			if (kind === "at") {
				var ts = new Date(schedAtInput.value).getTime();
				if (Number.isNaN(ts)) {
					schedAtInput.classList.add("field-error");
					return;
				}
				schedule = { kind: "at", atMs: ts };
			} else if (kind === "every") {
				var secs = parseInt(schedEveryInput.value, 10);
				if (Number.isNaN(secs) || secs <= 0) {
					schedEveryInput.classList.add("field-error");
					return;
				}
				schedule = { kind: "every", everyMs: secs * 1000 };
			} else {
				var expr = schedCronInput.value.trim();
				if (!expr) {
					schedCronInput.classList.add("field-error");
					return;
				}
				schedule = { kind: "cron", expr: expr };
				var tz = schedTzInput.value.trim();
				if (tz) schedule.tz = tz;
			}
			var msgText = payloadTextInput.value.trim();
			if (!msgText) {
				payloadTextInput.classList.add("field-error");
				return;
			}
			var payload =
				payloadSelect.value === "systemEvent"
					? { kind: "systemEvent", text: msgText }
					: { kind: "agentTurn", message: msgText, deliver: false };
			saveBtn.disabled = true;
			saveBtn.textContent = "Saving...";
			var rpcMethod = isEdit ? "cron.update" : "cron.add";
			var rpcParams = isEdit
				? {
						id: existingJob.id,
						patch: {
							name: name,
							schedule: schedule,
							payload: payload,
							sessionTarget: targetSelect.value,
							deleteAfterRun: deleteAfterCheck.checked,
							enabled: enabledCheck.checked,
						},
					}
				: {
						name: name,
						schedule: schedule,
						payload: payload,
						sessionTarget: targetSelect.value,
						deleteAfterRun: deleteAfterCheck.checked,
						enabled: enabledCheck.checked,
					};
			sendRpc(rpcMethod, rpcParams).then((res) => {
				if (res?.ok) {
					closeProviderModal();
					loadJobs();
					loadStatus();
				} else {
					saveBtn.disabled = false;
					saveBtn.textContent = isEdit ? "Update" : "Create";
				}
			});
		});
		btns.appendChild(saveBtn);
		form.appendChild(btns);
		providerModalBody.appendChild(form);
		nameInput.focus();
	}

	S.$("cronAddBtn").addEventListener("click", () => {
		openCronModal(null);
	});
	S.$("cronRefreshBtn").addEventListener("click", () => {
		loadJobs();
		loadStatus();
	});
	loadStatus();
	loadJobs();
});
