// ── Router ──────────────────────────────────────────────────

import { clearLogsAlert } from "./logs-alert.js";
import * as S from "./state.js";

var pages = {};
var prefixRoutes = [];
export var currentPage = null;
export var currentPrefix = null;

export function sessionPath(key) {
	return `/chats/${key.replace(/:/g, "/")}`;
}
var pageContent = S.$("pageContent");
var sessionsPanel = S.$("sessionsPanel");

export function registerPage(path, init, teardown) {
	pages[path] = { init: init, teardown: teardown || (() => {}) };
}

export function registerPrefix(prefix, init, teardown) {
	prefixRoutes.push({
		prefix: prefix,
		init: init,
		teardown: teardown || (() => {}),
	});
}

export function navigate(path) {
	if (path === currentPage) return;
	history.pushState(null, "", path);
	mount(path);
}

export function mount(path) {
	// Teardown previous page
	if (currentPage) {
		if (pages[currentPage]) {
			pages[currentPage].teardown();
		} else if (currentPrefix) {
			var prevPR = prefixRoutes.find((r) => r.prefix === currentPrefix);
			if (prevPR) prevPR.teardown();
		}
	}
	pageContent.textContent = "";
	currentPrefix = null;

	// Try exact match first
	var page = pages[path];
	var param = null;
	var matchedPrefix = null;

	if (!page) {
		// Try prefix routes
		for (var i = 0; i < prefixRoutes.length; i++) {
			var pr = prefixRoutes[i];
			if (path === pr.prefix || path.indexOf(`${pr.prefix}/`) === 0) {
				page = pr;
				matchedPrefix = pr.prefix;
				var suffix = path.substring(pr.prefix.length + 1);
				param = suffix ? decodeURIComponent(suffix.replace(/\//g, ":")) : null;
				break;
			}
		}
	}

	if (!page) {
		page = pages["/"];
	}

	currentPage = path;
	currentPrefix = matchedPrefix;

	// Nav link active state: match exact or prefix
	var links = document.querySelectorAll(".nav-link");
	links.forEach((a) => {
		var href = a.getAttribute("href");
		var active = href === path || (href !== "/" && path.indexOf(href) === 0);
		a.classList.toggle("active", active);
	});

	// Show sessions panel on chat pages
	if (matchedPrefix === "/chats" || path === "/chats") {
		sessionsPanel.classList.remove("hidden");
	} else {
		sessionsPanel.classList.add("hidden");
	}

	// Clear unseen logs alert when viewing the logs page
	if (path === "/logs") clearLogsAlert();

	if (page) page.init(pageContent, param);
}

window.addEventListener("popstate", () => {
	mount(location.pathname);
});

// ── Nav panel (burger toggle) ────────────────────────────────
var burgerBtn = S.$("burgerBtn");
var navPanel = S.$("navPanel");

burgerBtn.addEventListener("click", () => {
	navPanel.classList.toggle("hidden");
});

navPanel.addEventListener("click", (e) => {
	var link = e.target.closest("[data-nav]");
	if (!link) return;
	e.preventDefault();
	navigate(link.getAttribute("href"));
});
