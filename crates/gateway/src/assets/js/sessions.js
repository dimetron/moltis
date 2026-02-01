// ── Sessions: list, switch, status helpers ──────────────────

import {
	appendChannelFooter,
	chatAddMsg,
	highlightAndScroll,
	removeThinking,
	scrollChatToBottom,
	stripChannelPrefix,
	updateTokenBar,
} from "./chat-ui.js";
import { formatTokens, renderMarkdown, sendRpc } from "./helpers.js";
import { makeChatIcon, makeTelegramIcon } from "./icons.js";
import { updateSessionProjectSelect } from "./project-combo.js";
import { currentPrefix, navigate, sessionPath } from "./router.js";
import { updateSandboxUI } from "./sandbox.js";
import * as S from "./state.js";

// ── Fetch & render ──────────────────────────────────────────

export function fetchSessions() {
	sendRpc("sessions.list", {}).then((res) => {
		if (!res || !res.ok) return;
		S.setSessions(res.payload || []);
		renderSessionList();
	});
}

export function renderSessionList() {
	var sessionList = S.$("sessionList");
	sessionList.textContent = "";
	var filtered = S.sessions;
	if (S.projectFilterId) {
		filtered = S.sessions.filter((s) => s.projectId === S.projectFilterId);
	}
	filtered.forEach((s) => {
		var item = document.createElement("div");
		item.className = `session-item${s.key === S.activeSessionKey ? " active" : ""}`;
		item.setAttribute("data-session-key", s.key);

		var info = document.createElement("div");
		info.className = "session-info";

		var label = document.createElement("div");
		label.className = "session-label";
		label.style.display = "flex";
		label.style.alignItems = "center";
		label.style.gap = "5px";
		var iconWrap = document.createElement("span");
		iconWrap.className = "session-icon";
		var isTelegram = false;
		if (s.channelBinding) {
			try {
				var binding = JSON.parse(s.channelBinding);
				if (binding.channel_type === "telegram") isTelegram = true;
			} catch (_e) {
				/* ignore bad JSON */
			}
		}
		var icon = isTelegram ? makeTelegramIcon() : makeChatIcon();
		iconWrap.appendChild(icon);
		if (isTelegram) {
			iconWrap.style.color = s.activeChannel ? "var(--accent)" : "var(--muted)";
			iconWrap.style.opacity = s.activeChannel ? "1" : "0.5";
			iconWrap.title = s.activeChannel
				? "Active Telegram session"
				: "Telegram session (inactive)";
		} else {
			iconWrap.style.color = "var(--muted)";
		}
		var spinner = document.createElement("span");
		spinner.className = "session-spinner";
		iconWrap.appendChild(spinner);
		label.appendChild(iconWrap);
		var labelText = document.createElement("span");
		labelText.textContent = s.label || s.key;
		label.appendChild(labelText);
		info.appendChild(label);

		var meta = document.createElement("div");
		meta.className = "session-meta";
		meta.setAttribute("data-session-key", s.key);
		var count = s.messageCount || 0;
		var metaText = `${count} msg${count !== 1 ? "s" : ""}`;
		if (s.worktree_branch) {
			metaText += ` \u00b7 \u2387 ${s.worktree_branch}`;
		}
		meta.textContent = metaText;
		info.appendChild(meta);

		item.appendChild(info);

		var actions = document.createElement("div");
		actions.className = "session-actions";

		if (s.key !== "main") {
			if (!s.channelBinding) {
				var renameBtn = document.createElement("button");
				renameBtn.className = "session-action-btn";
				renameBtn.textContent = "\u270F";
				renameBtn.title = "Rename";
				renameBtn.addEventListener("click", (e) => {
					e.stopPropagation();
					var newLabel = prompt("Rename session:", s.label || s.key);
					if (newLabel !== null) {
						sendRpc("sessions.patch", { key: s.key, label: newLabel }).then(
							fetchSessions,
						);
					}
				});
				actions.appendChild(renameBtn);
			}

			var deleteBtn = document.createElement("button");
			deleteBtn.className = "session-action-btn session-delete";
			deleteBtn.textContent = "\u2715";
			deleteBtn.title = "Delete";
			deleteBtn.addEventListener("click", (e) => {
				e.stopPropagation();
				var metaEl = sessionList.querySelector(
					`.session-meta[data-session-key="${s.key}"]`,
				);
				var count = metaEl
					? parseInt(metaEl.textContent, 10) || 0
					: s.messageCount || 0;
				if (count > 0 && !confirm("Delete this session?")) return;
				sendRpc("sessions.delete", { key: s.key }).then((res) => {
					if (
						res &&
						!res.ok &&
						res.error &&
						res.error.indexOf("uncommitted changes") !== -1
					) {
						if (confirm("Worktree has uncommitted changes. Force delete?")) {
							sendRpc("sessions.delete", { key: s.key, force: true }).then(
								() => {
									if (S.activeSessionKey === s.key) switchSession("main");
									fetchSessions();
								},
							);
						}
						return;
					}
					if (S.activeSessionKey === s.key) switchSession("main");
					fetchSessions();
				});
			});
			actions.appendChild(deleteBtn);
		}
		item.appendChild(actions);

		item.addEventListener("click", () => {
			if (currentPrefix !== "/chats") {
				navigate(sessionPath(s.key));
			} else {
				switchSession(s.key);
			}
		});

		sessionList.appendChild(item);
	});
}

// ── Braille spinner for active sessions ─────────────────────
var spinnerFrames = [
	"\u280B",
	"\u2819",
	"\u2839",
	"\u2838",
	"\u283C",
	"\u2834",
	"\u2826",
	"\u2827",
	"\u2807",
	"\u280F",
];
var spinnerIndex = 0;
setInterval(() => {
	spinnerIndex = (spinnerIndex + 1) % spinnerFrames.length;
	var els = document.querySelectorAll(
		".session-item.replying .session-spinner",
	);
	for (var i = 0; i < els.length; i++)
		els[i].textContent = spinnerFrames[spinnerIndex];
}, 80);

// ── Status helpers ──────────────────────────────────────────

export function setSessionReplying(key, replying) {
	var sessionList = S.$("sessionList");
	var el = sessionList.querySelector(
		`.session-item[data-session-key="${key}"]`,
	);
	if (el) el.classList.toggle("replying", replying);
}

export function setSessionUnread(key, unread) {
	var sessionList = S.$("sessionList");
	var el = sessionList.querySelector(
		`.session-item[data-session-key="${key}"]`,
	);
	if (el) el.classList.toggle("unread", unread);
}

export function bumpSessionCount(key, increment) {
	var sessionList = S.$("sessionList");
	var el = sessionList.querySelector(
		`.session-meta[data-session-key="${key}"]`,
	);
	if (!el) return;
	var current = parseInt(el.textContent, 10) || 0;
	var next = current + increment;
	el.textContent = `${next} msg${next !== 1 ? "s" : ""}`;
}

// ── New session button ──────────────────────────────────────
var newSessionBtn = S.$("newSessionBtn");
newSessionBtn.addEventListener("click", () => {
	var key = `session:${crypto.randomUUID()}`;
	navigate(sessionPath(key));
});

// ── Switch session ──────────────────────────────────────────

export function switchSession(key, searchContext, projectId) {
	var sessionList = S.$("sessionList");
	S.setActiveSessionKey(key);
	localStorage.setItem("moltis-session", key);
	history.replaceState(null, "", sessionPath(key));
	if (S.chatMsgBox) S.chatMsgBox.textContent = "";
	S.setStreamEl(null);
	S.setStreamText("");
	S.setLastHistoryIndex(-1);
	S.setSessionTokens({ input: 0, output: 0 });
	S.setSessionContextWindow(0);
	updateTokenBar();

	var items = sessionList.querySelectorAll(".session-item");
	items.forEach((el) => {
		var isTarget = el.getAttribute("data-session-key") === key;
		el.classList.toggle("active", isTarget);
		if (isTarget) el.classList.remove("unread");
	});

	S.setSessionSwitchInProgress(true);
	var switchParams = { key: key };
	if (projectId) switchParams.project_id = projectId;
	sendRpc("sessions.switch", switchParams).then((res) => {
		if (res?.ok && res.payload) {
			var entry = res.payload.entry || {};
			// Restore the session's project binding.
			// If we explicitly passed a projectId (e.g. new session), keep it
			// even if the server response hasn't persisted it yet.
			var effectiveProjectId = entry.projectId || projectId || "";
			S.setActiveProjectId(effectiveProjectId);
			localStorage.setItem("moltis-project", S.activeProjectId);
			updateSessionProjectSelect(S.activeProjectId);
			// Restore per-session model
			if (entry.model && S.models.length > 0) {
				var found = S.models.find((m) => m.id === entry.model);
				if (found) {
					S.setSelectedModelId(found.id);
					if (S.modelComboLabel)
						S.modelComboLabel.textContent = found.displayName || found.id;
					localStorage.setItem("moltis-model", found.id);
				}
			}
			// Restore sandbox state
			updateSandboxUI(entry.sandbox_enabled !== false);
			var history = res.payload.history || [];
			var msgEls = [];
			S.setSessionTokens({ input: 0, output: 0 });
			S.setChatBatchLoading(true);
			history.forEach((msg) => {
				if (msg.role === "user") {
					var userContent = msg.content || "";
					if (msg.channel) userContent = stripChannelPrefix(userContent);
					var userEl = chatAddMsg("user", renderMarkdown(userContent), true);
					if (userEl && msg.channel) appendChannelFooter(userEl, msg.channel);
					msgEls.push(userEl);
				} else if (msg.role === "assistant") {
					var el = chatAddMsg(
						"assistant",
						renderMarkdown(msg.content || ""),
						true,
					);
					if (el && msg.model) {
						var ft = document.createElement("div");
						ft.className = "msg-model-footer";
						var ftText = msg.provider
							? `${msg.provider} / ${msg.model}`
							: msg.model;
						if (msg.inputTokens || msg.outputTokens) {
							ftText +=
								" \u00b7 " +
								formatTokens(msg.inputTokens || 0) +
								" in / " +
								formatTokens(msg.outputTokens || 0) +
								" out";
						}
						ft.textContent = ftText;
						el.appendChild(ft);
					}
					if (msg.inputTokens || msg.outputTokens) {
						S.sessionTokens.input += msg.inputTokens || 0;
						S.sessionTokens.output += msg.outputTokens || 0;
					}
					msgEls.push(el);
				} else {
					msgEls.push(null);
				}
			});
			S.setChatBatchLoading(false);
			S.setLastHistoryIndex(history.length > 0 ? history.length - 1 : -1);
			S.setSessionSwitchInProgress(false);
			// Fetch context window for the token bar percentage display.
			sendRpc("chat.context", {}).then((ctxRes) => {
				if (ctxRes?.ok && ctxRes.payload && ctxRes.payload.tokenUsage) {
					S.setSessionContextWindow(
						ctxRes.payload.tokenUsage.contextWindow || 0,
					);
				}
				updateTokenBar();
			});
			updateTokenBar();

			if (searchContext?.query && S.chatMsgBox) {
				highlightAndScroll(
					msgEls,
					searchContext.messageIndex,
					searchContext.query,
				);
			} else {
				scrollChatToBottom();
			}

			var item = sessionList.querySelector(
				`.session-item[data-session-key="${key}"]`,
			);
			if (item?.classList.contains("replying") && S.chatMsgBox) {
				removeThinking();
				var thinkEl = document.createElement("div");
				thinkEl.className = "msg assistant thinking";
				thinkEl.id = "thinkingIndicator";
				var thinkDots = document.createElement("span");
				thinkDots.className = "thinking-dots";
				// Safe: static hardcoded HTML, no user input
				thinkDots.innerHTML = "<span></span><span></span><span></span>";
				thinkEl.appendChild(thinkDots);
				S.chatMsgBox.appendChild(thinkEl);
				scrollChatToBottom();
			}
			if (
				!sessionList.querySelector(`.session-meta[data-session-key="${key}"]`)
			) {
				fetchSessions();
			}
		} else {
			S.setSessionSwitchInProgress(false);
		}
	});
}
