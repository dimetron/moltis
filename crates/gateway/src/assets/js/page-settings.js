// ── Settings page (Preact + HTM + Signals) ───────────────────

import { signal } from "@preact/signals";
import { html } from "htm/preact";
import { render } from "preact";
import { useEffect, useRef, useState } from "preact/hooks";
import { sendRpc } from "./helpers.js";
import { registerPage } from "./router.js";
import * as S from "./state.js";

var identity = signal(null);
var loading = signal(true);
var activeSection = signal("identity");
var mounted = false;
var containerRef = null;

function rerender() {
	if (containerRef) render(html`<${SettingsPage} />`, containerRef);
}

function fetchIdentity() {
	if (!mounted) return;
	sendRpc("agent.identity.get", {}).then((res) => {
		if (res?.ok) {
			identity.value = res.payload;
			loading.value = false;
			rerender();
		} else if (mounted && !S.connected) {
			setTimeout(fetchIdentity, 500);
		} else {
			loading.value = false;
			rerender();
		}
	});
}

// ── Sidebar navigation items ─────────────────────────────────

var sections = [
	{
		id: "identity",
		label: "Identity",
		icon: html`<svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" width="16" height="16"><path stroke-linecap="round" stroke-linejoin="round" d="M15.75 6a3.75 3.75 0 1 1-7.5 0 3.75 3.75 0 0 1 7.5 0ZM4.501 20.118a7.5 7.5 0 0 1 14.998 0A17.933 17.933 0 0 1 12 21.75c-2.676 0-5.216-.584-7.499-1.632Z"/></svg>`,
	},
];

function SettingsSidebar() {
	return html`<div class="settings-sidebar">
		<div class="settings-sidebar-nav">
			${sections.map(
				(s) => html`
				<button
					key=${s.id}
					class="settings-nav-item ${activeSection.value === s.id ? "active" : ""}"
					onClick=${() => {
						activeSection.value = s.id;
						rerender();
					}}
				>
					${s.icon}
					${s.label}
				</button>
			`,
			)}
		</div>
	</div>`;
}

// ── Emoji picker ─────────────────────────────────────────────

var EMOJI_LIST = [
	"\u{1f436}",
	"\u{1f431}",
	"\u{1f43b}",
	"\u{1f43a}",
	"\u{1f981}",
	"\u{1f985}",
	"\u{1f989}",
	"\u{1f427}",
	"\u{1f422}",
	"\u{1f40d}",
	"\u{1f409}",
	"\u{1f984}",
	"\u{1f419}",
	"\u{1f41d}",
	"\u{1f98a}",
	"\u{1f43f}\ufe0f",
	"\u{1f994}",
	"\u{1f987}",
	"\u{1f40a}",
	"\u{1f433}",
	"\u{1f42c}",
	"\u{1f99c}",
	"\u{1f9a9}",
	"\u{1f426}",
	"\u{1f40e}",
	"\u{1f98c}",
	"\u{1f418}",
	"\u{1f99b}",
	"\u{1f43c}",
	"\u{1f428}",
	"\u{1f916}",
	"\u{1f47e}",
	"\u{1f47b}",
	"\u{1f383}",
	"\u{2b50}",
	"\u{1f525}",
	"\u{26a1}",
	"\u{1f308}",
	"\u{1f31f}",
	"\u{1f4a1}",
	"\u{1f52e}",
	"\u{1f680}",
	"\u{1f30d}",
	"\u{1f335}",
	"\u{1f33b}",
	"\u{1f340}",
	"\u{1f344}",
	"\u{2744}\ufe0f",
];

function EmojiPicker({ value, onChange }) {
	var [open, setOpen] = useState(false);
	var wrapRef = useRef(null);

	useEffect(() => {
		if (!open) return;
		function onClick(e) {
			if (wrapRef.current && !wrapRef.current.contains(e.target)) setOpen(false);
		}
		document.addEventListener("mousedown", onClick);
		return () => document.removeEventListener("mousedown", onClick);
	}, [open]);

	return html`<div class="settings-emoji-field" ref=${wrapRef}>
		<input
			type="text"
			class="settings-input"
			style="width:3.5rem;text-align:center;font-size:1.3rem"
			value=${value || ""}
			onInput=${(e) => onChange(e.target.value)}
			placeholder="\u{1f43e}"
		/>
		<button
			type="button"
			class="settings-btn"
			style="padding:0.35rem 0.6rem;font-size:0.75rem"
			onClick=${() => setOpen(!open)}
		>
			${open ? "Close" : "Pick"}
		</button>
		${
			open
				? html`<div class="settings-emoji-picker">
				${EMOJI_LIST.map(
					(em) =>
						html`<button
							type="button"
							class="settings-emoji-btn ${value === em ? "active" : ""}"
							onClick=${() => {
								onChange(em);
								setOpen(false);
							}}
						>
							${em}
						</button>`,
				)}
			</div>`
				: null
		}
	</div>`;
}

// ── Soul defaults ────────────────────────────────────────────

var DEFAULT_SOUL =
	"Be genuinely helpful, not performatively helpful. Skip the filler words \u2014 just help.\n" +
	"Have opinions. You're allowed to disagree, prefer things, find stuff amusing or boring.\n" +
	"Be resourceful before asking. Try to figure it out first \u2014 read the context, search for it \u2014 then ask if you're stuck.\n" +
	"Earn trust through competence. Be careful with external actions. Be bold with internal ones.\n" +
	"Remember you're a guest. You have access to someone's life. Treat it with respect.\n" +
	"Private things stay private. When in doubt, ask before acting externally.\n" +
	"Be concise when needed, thorough when it matters. Not a corporate drone. Not a sycophant. Just good.";

// ── Identity section (editable form) ─────────────────────────

function IdentitySection() {
	var id = identity.value;
	var isNew = !(id && (id.name || id.user_name));

	var [name, setName] = useState(id?.name || "");
	var [emoji, setEmoji] = useState(id?.emoji || "");
	var [creature, setCreature] = useState(id?.creature || "");
	var [vibe, setVibe] = useState(id?.vibe || "");
	var [userName, setUserName] = useState(id?.user_name || "");
	var [soul, setSoul] = useState(id?.soul || "");
	var [saving, setSaving] = useState(false);
	var [saved, setSaved] = useState(false);
	var [error, setError] = useState(null);

	if (loading.value) {
		return html`<div class="settings-content">
			<p class="text-sm text-[var(--muted)]">Loading...</p>
		</div>`;
	}

	function onSave(e) {
		e.preventDefault();
		if (!(name.trim() || userName.trim())) {
			setError("Agent name and your name are required.");
			return;
		}
		if (!name.trim()) {
			setError("Agent name is required.");
			return;
		}
		if (!userName.trim()) {
			setError("Your name is required.");
			return;
		}
		setError(null);
		setSaving(true);
		setSaved(false);

		sendRpc("agent.identity.update", {
			name: name.trim(),
			emoji: emoji.trim() || "",
			creature: creature.trim() || "",
			vibe: vibe.trim() || "",
			soul: soul.trim() || null,
			user_name: userName.trim(),
		}).then((res) => {
			setSaving(false);
			if (res?.ok) {
				identity.value = res.payload;
				setSaved(true);
				setTimeout(() => {
					setSaved(false);
					rerender();
				}, 2000);
			} else {
				setError(res?.error?.message || "Failed to save");
			}
			rerender();
		});
	}

	function onResetSoul() {
		setSoul("");
		rerender();
	}

	return html`<div class="settings-content">
		<h2 class="settings-title">Identity</h2>
		${
			isNew
				? html`<p class="settings-hint" style="margin-bottom:1rem">
				Welcome! Set up your agent's identity to get started.
			</p>`
				: null
		}
		<form onSubmit=${onSave}>
			<div class="settings-section">
				<h3 class="settings-section-title">Agent</h3>
				<div class="settings-grid">
					<div class="settings-field">
						<label class="settings-label">Name *</label>
						<input
							type="text"
							class="settings-input"
							value=${name}
							onInput=${(e) => setName(e.target.value)}
							placeholder="e.g. Rex"
						/>
					</div>
					<div class="settings-field">
						<label class="settings-label">Emoji</label>
						<${EmojiPicker} value=${emoji} onChange=${setEmoji} />
					</div>
					<div class="settings-field">
						<label class="settings-label">Creature</label>
						<input
							type="text"
							class="settings-input"
							value=${creature}
							onInput=${(e) => setCreature(e.target.value)}
							placeholder="e.g. dog"
						/>
					</div>
					<div class="settings-field">
						<label class="settings-label">Vibe</label>
						<input
							type="text"
							class="settings-input"
							value=${vibe}
							onInput=${(e) => setVibe(e.target.value)}
							placeholder="e.g. chill"
						/>
					</div>
				</div>
			</div>
			<div class="settings-section">
				<h3 class="settings-section-title">User</h3>
				<div class="settings-grid">
					<div class="settings-field">
						<label class="settings-label">Your name *</label>
						<input
							type="text"
							class="settings-input"
							value=${userName}
							onInput=${(e) => setUserName(e.target.value)}
							placeholder="e.g. Alice"
						/>
					</div>
				</div>
			</div>
			<div class="settings-section">
				<h3 class="settings-section-title">Soul</h3>
				<p class="settings-hint">Personality and tone injected into every conversation. Leave empty for the default.</p>
				<textarea
					class="settings-textarea"
					rows="8"
					placeholder=${DEFAULT_SOUL}
					value=${soul}
					onInput=${(e) => setSoul(e.target.value)}
				/>
				${
					soul
						? html`<div style="margin-top:0.25rem">
						<button type="button" class="settings-btn settings-btn-secondary" onClick=${onResetSoul}>Reset to default</button>
					</div>`
						: null
				}
			</div>
			<div class="settings-actions">
				<button type="submit" class="settings-btn" disabled=${saving}>
					${saving ? "Saving\u2026" : "Save"}
				</button>
				${saved ? html`<span class="settings-saved">Saved</span>` : null}
				${error ? html`<span class="settings-error">${error}</span>` : null}
			</div>
		</form>
	</div>`;
}

// ── Main layout ──────────────────────────────────────────────

function SettingsPage() {
	useEffect(() => {
		fetchIdentity();
	}, []);

	var section = activeSection.value;

	return html`<div class="settings-layout">
		<${SettingsSidebar} />
		${section === "identity" ? html`<${IdentitySection} />` : null}
	</div>`;
}

registerPage(
	"/settings",
	(container) => {
		mounted = true;
		containerRef = container;
		container.style.cssText = "flex-direction:row;padding:0;overflow:hidden;";
		render(html`<${SettingsPage} />`, container);
		fetchIdentity();
	},
	() => {
		mounted = false;
		if (containerRef) render(null, containerRef);
		containerRef = null;
		identity.value = null;
		loading.value = true;
		activeSection.value = "identity";
	},
);
