// ── Channels page ───────────────────────────────────────────

import { onEvent } from "./events.js";
import { createEl, sendRpc } from "./helpers.js";
import { makeTelegramIcon } from "./icons.js";
import { registerPage } from "./router.js";
import * as S from "./state.js";

export function prefetchChannels() {
	sendRpc("channels.status", {}).then((res) => {
		if (res?.ok) {
			S.setCachedChannels(res.payload?.channels || []);
		}
	});
}

var channelModal = S.$("channelModal");
var channelModalTitle = S.$("channelModalTitle");
var channelModalBody = S.$("channelModalBody");
var channelModalClose = S.$("channelModalClose");

function openChannelModal(onAdded) {
	channelModal.classList.remove("hidden");
	channelModalTitle.textContent = "Add Telegram Bot";
	channelModalBody.textContent = "";

	var form = createEl("div", { className: "channel-form" });

	var helpBox = createEl("div", { className: "channel-card" });
	var helpTitle = createEl("span", {
		className: "text-xs font-medium text-[var(--text-strong)]",
		textContent: "How to create a Telegram bot",
	});
	helpBox.appendChild(helpTitle);

	var step1 = createEl("div", {
		className: "text-xs text-[var(--muted)] channel-help",
	});
	step1.appendChild(document.createTextNode("1. Open "));
	var bfLink = createEl("a", {
		href: "https://t.me/BotFather",
		target: "_blank",
		className: "text-[var(--accent)]",
		style: "text-decoration:underline;",
		textContent: "@BotFather",
	});
	step1.appendChild(bfLink);
	step1.appendChild(document.createTextNode(" in Telegram"));
	helpBox.appendChild(step1);

	helpBox.appendChild(
		createEl("div", {
			className: "text-xs text-[var(--muted)]",
			textContent:
				"2. Send /newbot and follow the prompts to choose a name and username",
		}),
	);
	helpBox.appendChild(
		createEl("div", {
			className: "text-xs text-[var(--muted)]",
			textContent:
				"3. Copy the bot token (looks like 123456:ABC-DEF...) and paste it below",
		}),
	);

	var helpTip = createEl("div", {
		className: "text-xs text-[var(--muted)] channel-help",
		style: "margin-top:2px;",
	});
	helpTip.appendChild(document.createTextNode("See the "));
	var docsLink = createEl("a", {
		href: "https://core.telegram.org/bots/tutorial",
		target: "_blank",
		className: "text-[var(--accent)]",
		style: "text-decoration:underline;",
		textContent: "Telegram Bot Tutorial",
	});
	helpTip.appendChild(docsLink);
	helpTip.appendChild(document.createTextNode(" for more details."));
	helpBox.appendChild(helpTip);

	form.appendChild(helpBox);

	var idLabel = createEl("label", {
		className: "text-xs text-[var(--muted)]",
		textContent: "Bot username",
	});
	var idInput = createEl("input", {
		type: "text",
		placeholder: "e.g. my_assistant_bot",
		className:
			"text-sm bg-[var(--surface2)] border border-[var(--border)] rounded px-3 py-2 text-[var(--text)] focus:outline-none focus:border-[var(--border-strong)]",
		style: "font-family:var(--font-body);",
	});

	var tokenLabel = createEl("label", {
		className: "text-xs text-[var(--muted)]",
		textContent: "Bot Token (from @BotFather)",
	});
	var tokenInput = createEl("input", {
		type: "password",
		placeholder: "123456:ABC-DEF...",
		className:
			"text-sm bg-[var(--surface2)] border border-[var(--border)] rounded px-3 py-2 text-[var(--text)] focus:outline-none focus:border-[var(--border-strong)]",
		style: "font-family:var(--font-body);",
	});

	var dmLabel = createEl("label", {
		className: "text-xs text-[var(--muted)]",
		textContent: "DM Policy",
	});
	var dmSelect = createEl("select", {
		className:
			"text-sm bg-[var(--surface2)] border border-[var(--border)] rounded px-3 py-2 text-[var(--text)] cursor-pointer",
		style: "font-family:var(--font-body);",
	});
	[
		["open", "Open (anyone)"],
		["allowlist", "Allowlist only"],
		["disabled", "Disabled"],
	].forEach((opt) => {
		dmSelect.appendChild(
			createEl("option", { value: opt[0], textContent: opt[1] }),
		);
	});

	var mentionLabel = createEl("label", {
		className: "text-xs text-[var(--muted)]",
		textContent: "Group Mention Mode",
	});
	var mentionSelect = createEl("select", {
		className:
			"text-sm bg-[var(--surface2)] border border-[var(--border)] rounded px-3 py-2 text-[var(--text)] cursor-pointer",
		style: "font-family:var(--font-body);",
	});
	[
		["mention", "Must @mention bot"],
		["always", "Always respond"],
		["none", "Don't respond in groups"],
	].forEach((opt) => {
		mentionSelect.appendChild(
			createEl("option", { value: opt[0], textContent: opt[1] }),
		);
	});

	var modelLabel = createEl("label", {
		className: "text-xs text-[var(--muted)]",
		textContent: "Default Model",
	});
	var modelSelect = createEl("select", {
		className:
			"text-sm bg-[var(--surface2)] border border-[var(--border)] rounded px-3 py-2 text-[var(--text)] cursor-pointer",
		style: "font-family:var(--font-body);",
	});
	modelSelect.appendChild(
		createEl("option", { value: "", textContent: "(server default)" }),
	);
	S.models.forEach((m) => {
		modelSelect.appendChild(
			createEl("option", { value: m.id, textContent: m.displayName || m.id }),
		);
	});

	var allowLabel = createEl("label", {
		className: "text-xs text-[var(--muted)]",
		textContent: "DM Allowlist (one username per line)",
	});
	var allowInput = createEl("textarea", {
		placeholder: "user1\nuser2",
		rows: 3,
		className:
			"text-sm bg-[var(--surface2)] border border-[var(--border)] rounded px-3 py-2 text-[var(--text)] focus:outline-none focus:border-[var(--border-strong)]",
		style: "font-family:var(--font-body);resize:vertical;",
	});

	var errorEl = createEl("div", {
		className: "text-xs text-[var(--error)] channel-error",
	});

	var submitBtn = createEl("button", {
		className:
			"bg-[var(--accent-dim)] text-white border-none px-4 py-2 rounded text-sm cursor-pointer hover:bg-[var(--accent)] transition-colors",
		textContent: "Connect Bot",
	});

	submitBtn.addEventListener("click", () => {
		var accountId = idInput.value.trim();
		var token = tokenInput.value.trim();
		if (!accountId) {
			errorEl.textContent = "Bot username is required.";
			errorEl.style.display = "block";
			return;
		}
		if (!token) {
			errorEl.textContent = "Bot token is required.";
			errorEl.style.display = "block";
			return;
		}

		var allowlist = allowInput.value
			.trim()
			.split(/\n/)
			.map((s) => s.trim())
			.filter(Boolean);
		errorEl.style.display = "none";
		submitBtn.disabled = true;
		submitBtn.textContent = "Connecting...";

		var addConfig = {
			token: token,
			dm_policy: dmSelect.value,
			mention_mode: mentionSelect.value,
			allowlist: allowlist,
		};
		if (modelSelect.value) addConfig.model = modelSelect.value;

		sendRpc("channels.add", {
			type: "telegram",
			account_id: accountId,
			config: addConfig,
		}).then((res) => {
			submitBtn.disabled = false;
			submitBtn.textContent = "Connect Bot";
			if (res?.ok) {
				closeChannelModal();
				if (onAdded) onAdded();
			} else {
				var msg =
					(res?.error && (res.error.message || res.error.detail)) ||
					"Failed to connect bot.";
				errorEl.textContent = msg;
				errorEl.style.display = "block";
			}
		});
	});

	form.appendChild(idLabel);
	form.appendChild(idInput);
	form.appendChild(tokenLabel);
	form.appendChild(tokenInput);
	form.appendChild(dmLabel);
	form.appendChild(dmSelect);
	form.appendChild(mentionLabel);
	form.appendChild(mentionSelect);
	form.appendChild(modelLabel);
	form.appendChild(modelSelect);
	form.appendChild(allowLabel);
	form.appendChild(allowInput);
	form.appendChild(errorEl);
	form.appendChild(submitBtn);
	channelModalBody.appendChild(form);
	idInput.focus();
}

function closeChannelModal() {
	channelModal.classList.add("hidden");
}

function openEditChannelModal(ch, onUpdated) {
	channelModal.classList.remove("hidden");
	channelModalTitle.textContent = "Edit Telegram Bot";
	channelModalBody.textContent = "";

	var cfg = ch.config || {};
	var form = createEl("div", { className: "channel-form" });

	form.appendChild(
		createEl("div", {
			className: "text-sm text-[var(--text-strong)]",
			textContent: ch.name || ch.account_id,
		}),
	);

	var dmLabel = createEl("label", {
		className: "text-xs text-[var(--muted)]",
		textContent: "DM Policy",
	});
	var dmSelect = createEl("select", {
		className:
			"text-sm bg-[var(--surface2)] border border-[var(--border)] rounded px-3 py-2 text-[var(--text)] cursor-pointer",
		style: "font-family:var(--font-body);",
	});
	[
		["open", "Open (anyone)"],
		["allowlist", "Allowlist only"],
		["disabled", "Disabled"],
	].forEach((opt) => {
		var o = createEl("option", { value: opt[0], textContent: opt[1] });
		if (opt[0] === cfg.dm_policy) o.selected = true;
		dmSelect.appendChild(o);
	});

	var mentionLabel = createEl("label", {
		className: "text-xs text-[var(--muted)]",
		textContent: "Group Mention Mode",
	});
	var mentionSelect = createEl("select", {
		className:
			"text-sm bg-[var(--surface2)] border border-[var(--border)] rounded px-3 py-2 text-[var(--text)] cursor-pointer",
		style: "font-family:var(--font-body);",
	});
	[
		["mention", "Must @mention bot"],
		["always", "Always respond"],
		["none", "Don't respond in groups"],
	].forEach((opt) => {
		var o = createEl("option", { value: opt[0], textContent: opt[1] });
		if (opt[0] === cfg.mention_mode) o.selected = true;
		mentionSelect.appendChild(o);
	});

	var editModelLabel = createEl("label", {
		className: "text-xs text-[var(--muted)]",
		textContent: "Default Model",
	});
	var editModelSelect = createEl("select", {
		className:
			"text-sm bg-[var(--surface2)] border border-[var(--border)] rounded px-3 py-2 text-[var(--text)] cursor-pointer",
		style: "font-family:var(--font-body);",
	});
	editModelSelect.appendChild(
		createEl("option", { value: "", textContent: "(server default)" }),
	);
	S.models.forEach((m) => {
		var o = createEl("option", {
			value: m.id,
			textContent: m.displayName || m.id,
		});
		if (m.id === cfg.model) o.selected = true;
		editModelSelect.appendChild(o);
	});

	var allowLabel = createEl("label", {
		className: "text-xs text-[var(--muted)]",
		textContent: "DM Allowlist (one username per line)",
	});
	var allowInput = createEl("textarea", {
		rows: 3,
		className:
			"text-sm bg-[var(--surface2)] border border-[var(--border)] rounded px-3 py-2 text-[var(--text)] focus:outline-none focus:border-[var(--border-strong)]",
		style: "font-family:var(--font-body);resize:vertical;",
	});
	allowInput.value = (cfg.allowlist || []).join("\n");

	var errorEl = createEl("div", {
		className: "text-xs text-[var(--error)] channel-error",
	});

	var saveBtn = createEl("button", {
		className:
			"bg-[var(--accent-dim)] text-white border-none px-4 py-2 rounded text-sm cursor-pointer hover:bg-[var(--accent)] transition-colors",
		textContent: "Save Changes",
	});
	saveBtn.addEventListener("click", () => {
		var allowlist = allowInput.value
			.trim()
			.split(/\n/)
			.map((s) => s.trim())
			.filter(Boolean);
		errorEl.style.display = "none";
		saveBtn.disabled = true;
		saveBtn.textContent = "Saving...";
		var updateConfig = {
			token: cfg.token || "",
			dm_policy: dmSelect.value,
			mention_mode: mentionSelect.value,
			allowlist: allowlist,
		};
		if (editModelSelect.value) updateConfig.model = editModelSelect.value;
		sendRpc("channels.update", {
			account_id: ch.account_id,
			config: updateConfig,
		}).then((res) => {
			saveBtn.disabled = false;
			saveBtn.textContent = "Save Changes";
			if (res?.ok) {
				closeChannelModal();
				if (onUpdated) onUpdated();
			} else {
				var msg =
					(res?.error && (res.error.message || res.error.detail)) ||
					"Failed to update bot.";
				errorEl.textContent = msg;
				errorEl.style.display = "block";
			}
		});
	});

	form.appendChild(dmLabel);
	form.appendChild(dmSelect);
	form.appendChild(mentionLabel);
	form.appendChild(mentionSelect);
	form.appendChild(editModelLabel);
	form.appendChild(editModelSelect);
	form.appendChild(allowLabel);
	form.appendChild(allowInput);
	form.appendChild(errorEl);
	form.appendChild(saveBtn);
	channelModalBody.appendChild(form);
}

channelModalClose.addEventListener("click", closeChannelModal);
channelModal.addEventListener("click", (e) => {
	if (e.target === channelModal) closeChannelModal();
});

// Safe: static hardcoded HTML template, no user input.
var channelsPageHTML =
	'<div class="flex-1 flex flex-col min-w-0 p-4 gap-4 overflow-y-auto">' +
	'<div class="flex items-center gap-3">' +
	'<h2 class="text-lg font-medium text-[var(--text-strong)]">Channels</h2>' +
	'<div style="display:flex;gap:4px;margin-left:12px;">' +
	'<button id="chanTabChannels" class="session-action-btn" style="font-weight:600;">Channels</button>' +
	'<button id="chanTabSenders" class="session-action-btn">Senders</button>' +
	"</div>" +
	'<button id="chanAddBtn" class="bg-[var(--accent-dim)] text-white border-none px-3 py-1.5 rounded text-xs cursor-pointer hover:bg-[var(--accent)] transition-colors">+ Add Telegram Bot</button>' +
	"</div>" +
	'<div id="channelPageList"></div>' +
	'<div id="sendersPageContent" style="display:none;">' +
	'<div style="margin-bottom:12px;">' +
	'<label class="text-xs text-[var(--muted)]" style="margin-right:6px;">Account:</label>' +
	'<select id="sendersAccountSelect" style="background:var(--surface2);color:var(--text);border:1px solid var(--border);border-radius:4px;padding:4px 8px;font-size:12px;"></select>' +
	"</div>" +
	'<div id="sendersTableWrap"></div>' +
	"</div>" +
	"</div>";

registerPage(
	"/channels",
	function initChannels(container) {
		container.innerHTML = channelsPageHTML; // safe: static template

		var addBtn = S.$("chanAddBtn");
		var listEl = S.$("channelPageList");
		var sendersContent = S.$("sendersPageContent");
		var tabChannels = S.$("chanTabChannels");
		var tabSenders = S.$("chanTabSenders");
		var sendersSelect = S.$("sendersAccountSelect");
		var sendersTableWrap = S.$("sendersTableWrap");
		var activeTab = "channels";

		S.setChannelEventUnsub(
			onEvent("channel", (p) => {
				if (
					p.kind === "inbound_message" &&
					activeTab === "senders" &&
					sendersSelect.value === p.account_id
				) {
					loadSenders();
				}
			}),
		);

		function switchTab(tab) {
			activeTab = tab;
			if (tab === "channels") {
				listEl.style.display = "";
				sendersContent.style.display = "none";
				addBtn.style.display = "";
				tabChannels.style.fontWeight = "600";
				tabSenders.style.fontWeight = "";
				renderChannelList();
			} else {
				listEl.style.display = "none";
				sendersContent.style.display = "";
				addBtn.style.display = "none";
				tabChannels.style.fontWeight = "";
				tabSenders.style.fontWeight = "600";
				loadSendersAccounts();
			}
		}

		tabChannels.addEventListener("click", () => {
			switchTab("channels");
		});
		tabSenders.addEventListener("click", () => {
			switchTab("senders");
		});
		addBtn.addEventListener("click", () => {
			if (S.connected) openChannelModal(renderChannelList);
		});

		function loadSendersAccounts() {
			sendRpc("channels.status", {}).then((res) => {
				if (!res || !res.ok) return;
				var channels = res.payload?.channels || [];
				while (sendersSelect.firstChild)
					sendersSelect.removeChild(sendersSelect.firstChild);
				if (channels.length === 0) {
					sendersTableWrap.textContent = "No channels configured.";
					return;
				}
				channels.forEach((ch) => {
					var opt = document.createElement("option");
					opt.value = ch.account_id;
					opt.textContent = ch.name || ch.account_id;
					sendersSelect.appendChild(opt);
				});
				loadSenders();
			});
		}

		sendersSelect.addEventListener("change", loadSenders);

		function loadSenders() {
			var accountId = sendersSelect.value;
			if (!accountId) return;
			sendRpc("channels.senders.list", { account_id: accountId }).then(
				(res) => {
					if (!res || !res.ok) {
						sendersTableWrap.textContent = "Failed to load senders.";
						return;
					}
					var senders = res.payload?.senders || [];
					while (sendersTableWrap.firstChild)
						sendersTableWrap.removeChild(sendersTableWrap.firstChild);

					if (senders.length === 0) {
						sendersTableWrap.appendChild(
							createEl("div", {
								className: "text-sm text-[var(--muted)] senders-empty",
								textContent: "No messages received yet for this account.",
							}),
						);
						return;
					}

					var table = createEl("table", { className: "senders-table" });
					var thead = document.createElement("thead");
					var headerRow = document.createElement("tr");
					[
						"Sender",
						"Username",
						"Messages",
						"Last Seen",
						"Status",
						"Action",
					].forEach((h) => {
						headerRow.appendChild(
							createEl("th", { textContent: h, className: "senders-th" }),
						);
					});
					thead.appendChild(headerRow);
					table.appendChild(thead);

					var tbody = document.createElement("tbody");
					senders.forEach((s) => {
						var tr = document.createElement("tr");
						tr.appendChild(
							createEl("td", {
								className: "senders-td",
								textContent: s.sender_name || s.peer_id,
							}),
						);
						tr.appendChild(
							createEl("td", {
								className: "senders-td",
								style: "color:var(--muted);",
								textContent: s.username ? `@${s.username}` : "\u2014",
							}),
						);
						tr.appendChild(
							createEl("td", {
								className: "senders-td",
								textContent: String(s.message_count),
							}),
						);
						var lastSeen = s.last_seen
							? new Date(s.last_seen * 1000).toLocaleString()
							: "\u2014";
						tr.appendChild(
							createEl("td", {
								className: "senders-td",
								style: "color:var(--muted);font-size:12px;",
								textContent: lastSeen,
							}),
						);

						var statusTd = createEl("td", { className: "senders-td" });
						statusTd.appendChild(
							createEl("span", {
								className: `provider-item-badge ${s.allowed ? "configured" : "oauth"}`,
								textContent: s.allowed ? "Allowed" : "Denied",
							}),
						);
						tr.appendChild(statusTd);

						var actionTd = createEl("td", { className: "senders-td" });
						var identifier = s.username || s.peer_id;
						if (s.allowed) {
							var denyBtn = createEl("button", {
								className: "session-action-btn session-delete",
								textContent: "Deny",
								title: "Remove from allowlist",
							});
							denyBtn.addEventListener("click", () => {
								sendRpc("channels.senders.deny", {
									account_id: accountId,
									identifier: identifier,
								}).then(() => {
									loadSenders();
								});
							});
							actionTd.appendChild(denyBtn);
						} else {
							var approveBtn = createEl("button", {
								className: "session-action-btn",
								textContent: "Approve",
								title: "Add to allowlist",
							});
							approveBtn.style.cssText =
								"background:var(--accent-dim);color:white;";
							approveBtn.addEventListener("click", () => {
								sendRpc("channels.senders.approve", {
									account_id: accountId,
									identifier: identifier,
								}).then(() => {
									loadSenders();
								});
							});
							actionTd.appendChild(approveBtn);
						}
						tr.appendChild(actionTd);
						tbody.appendChild(tr);
					});
					table.appendChild(tbody);
					sendersTableWrap.appendChild(table);
				},
			);
		}

		function renderChannelList() {
			// Use prefetched cache for instant render, then refresh in background
			if (S.cachedChannels !== null) {
				renderChannels(S.cachedChannels);
			}
			sendRpc("channels.status", {}).then((res) => {
				if (!res || !res.ok) return;
				var channels = res.payload?.channels || [];
				S.setCachedChannels(channels);
				renderChannels(channels);
			});
		}

		function renderChannels(channels) {
			while (listEl.firstChild) listEl.removeChild(listEl.firstChild);

			if (channels.length === 0) {
				var empty = createEl("div", {
					style: "text-align:center;padding:40px 0;",
				});
				empty.appendChild(
					createEl("div", {
						className: "text-sm text-[var(--muted)]",
						style: "margin-bottom:12px;",
						textContent: "No Telegram bots connected.",
					}),
				);
				empty.appendChild(
					createEl("div", {
						className: "text-xs text-[var(--muted)]",
						textContent:
							'Click "+ Add Telegram Bot" to connect one using a token from @BotFather.',
					}),
				);
				listEl.appendChild(empty);
				return;
			}

			channels.forEach((ch) => {
				var card = createEl("div", {
					className: "provider-card",
					style: "padding:12px 14px;border-radius:8px;margin-bottom:8px;",
				});
				var left = createEl("div", {
					style: "display:flex;align-items:center;gap:10px;",
				});
				var icon = createEl("span", {
					style:
						"display:inline-flex;align-items:center;justify-content:center;width:28px;height:28px;border-radius:6px;background:var(--surface2);",
				});
				icon.appendChild(makeTelegramIcon());
				left.appendChild(icon);

				var info = createEl("div", {
					style: "display:flex;flex-direction:column;gap:2px;",
				});
				info.appendChild(
					createEl("span", {
						className: "text-sm text-[var(--text-strong)]",
						textContent: ch.name || ch.account_id || "Telegram",
					}),
				);
				if (ch.details)
					info.appendChild(
						createEl("span", {
							className: "text-xs text-[var(--muted)]",
							textContent: ch.details,
						}),
					);
				if (ch.sessions && ch.sessions.length > 0) {
					var active = ch.sessions.filter((s) => s.active);
					var sessionLine =
						active.length > 0
							? active
									.map((s) => `${s.label || s.key} (${s.messageCount} msgs)`)
									.join(", ")
							: "No active session";
					info.appendChild(
						createEl("span", {
							className: "text-xs text-[var(--muted)]",
							textContent: sessionLine,
						}),
					);
				}
				left.appendChild(info);

				var statusClass = ch.status === "connected" ? "configured" : "oauth";
				left.appendChild(
					createEl("span", {
						className: `provider-item-badge ${statusClass}`,
						textContent: ch.status || "unknown",
					}),
				);
				card.appendChild(left);

				var actions = createEl("div", { style: "display:flex;gap:6px;" });
				var editBtn = createEl("button", {
					className: "session-action-btn",
					textContent: "Edit",
					title: `Edit ${ch.account_id || "channel"}`,
				});
				editBtn.addEventListener("click", () => {
					openEditChannelModal(ch, renderChannelList);
				});
				actions.appendChild(editBtn);

				var removeBtn = createEl("button", {
					className: "session-action-btn session-delete",
					textContent: "Remove",
					title: `Remove ${ch.account_id || "channel"}`,
				});
				removeBtn.addEventListener("click", () => {
					if (!confirm(`Remove ${ch.name || ch.account_id}?`)) return;
					sendRpc("channels.remove", { account_id: ch.account_id }).then(
						(r) => {
							if (r?.ok) renderChannelList();
						},
					);
				});
				actions.appendChild(removeBtn);
				card.appendChild(actions);
				listEl.appendChild(card);
			});
		}

		S.setRefreshChannelsPage(renderChannelList);
		renderChannelList();
	},
	function teardownChannels() {
		S.setRefreshChannelsPage(null);
		if (S.channelEventUnsub) {
			S.channelEventUnsub();
			S.setChannelEventUnsub(null);
		}
	},
);
