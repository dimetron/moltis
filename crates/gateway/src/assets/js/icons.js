// ── Shared icons ────────────────────────────────────────────
export function makeTelegramIcon() {
	var ns = "http://www.w3.org/2000/svg";
	var svg = document.createElementNS(ns, "svg");
	svg.setAttribute("width", "16");
	svg.setAttribute("height", "16");
	svg.setAttribute("viewBox", "0 0 24 24");
	svg.setAttribute("fill", "none");
	svg.setAttribute("stroke", "currentColor");
	svg.setAttribute("stroke-width", "1.5");
	var path = document.createElementNS(ns, "path");
	path.setAttribute("d", "M22 2L11 13M22 2l-7 20-4-9-9-4 20-7z");
	svg.appendChild(path);
	return svg;
}

export function makeCronIcon() {
	var ns = "http://www.w3.org/2000/svg";
	var svg = document.createElementNS(ns, "svg");
	svg.setAttribute("width", "16");
	svg.setAttribute("height", "16");
	svg.setAttribute("viewBox", "0 0 24 24");
	svg.setAttribute("fill", "none");
	svg.setAttribute("stroke", "currentColor");
	svg.setAttribute("stroke-width", "1.5");
	var path = document.createElementNS(ns, "path");
	path.setAttribute("stroke-linecap", "round");
	path.setAttribute("stroke-linejoin", "round");
	path.setAttribute("d", "M12 6v6h4.5m4.5 0a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z");
	svg.appendChild(path);
	return svg;
}

export function makeBranchIcon() {
	var ns = "http://www.w3.org/2000/svg";
	var svg = document.createElementNS(ns, "svg");
	svg.setAttribute("width", "16");
	svg.setAttribute("height", "16");
	svg.setAttribute("viewBox", "0 0 24 24");
	svg.setAttribute("fill", "none");
	svg.setAttribute("stroke", "currentColor");
	svg.setAttribute("stroke-width", "1.5");
	var path = document.createElementNS(ns, "path");
	path.setAttribute("stroke-linecap", "round");
	path.setAttribute("stroke-linejoin", "round");
	// Git-branch style: vertical line with a branch splitting off
	path.setAttribute(
		"d",
		"M6 3v12m0 0a3 3 0 1 0 3 3m-3-3a3 3 0 0 1 3-3h6a3 3 0 0 0 3-3V3m0 0a3 3 0 1 0-3 3m3-3a3 3 0 0 1-3 3",
	);
	svg.appendChild(path);
	return svg;
}

export function makeForkIcon() {
	var ns = "http://www.w3.org/2000/svg";
	var svg = document.createElementNS(ns, "svg");
	svg.setAttribute("width", "14");
	svg.setAttribute("height", "14");
	svg.setAttribute("viewBox", "0 0 24 24");
	svg.setAttribute("fill", "none");
	svg.setAttribute("stroke", "currentColor");
	svg.setAttribute("stroke-width", "1.5");
	var path = document.createElementNS(ns, "path");
	path.setAttribute("stroke-linecap", "round");
	path.setAttribute("stroke-linejoin", "round");
	path.setAttribute(
		"d",
		"M6 3v12m0 0a3 3 0 1 0 3 3m-3-3a3 3 0 0 1 3-3h6a3 3 0 0 0 3-3V3m0 0a3 3 0 1 0-3 3m3-3a3 3 0 0 1-3 3",
	);
	svg.appendChild(path);
	return svg;
}

export function makeChatIcon() {
	var ns = "http://www.w3.org/2000/svg";
	var svg = document.createElementNS(ns, "svg");
	svg.setAttribute("width", "16");
	svg.setAttribute("height", "16");
	svg.setAttribute("viewBox", "0 0 24 24");
	svg.setAttribute("fill", "none");
	svg.setAttribute("stroke", "currentColor");
	svg.setAttribute("stroke-width", "1.5");
	var path = document.createElementNS(ns, "path");
	path.setAttribute("stroke-linecap", "round");
	path.setAttribute("stroke-linejoin", "round");
	path.setAttribute(
		"d",
		"M7.5 8.25h9m-9 3H12m-9.75 1.51c0 1.6 1.123 2.994 2.707 3.227 1.087.16 2.185.283 3.293.369V21l4.076-4.076a1.526 1.526 0 0 1 1.037-.443 48.282 48.282 0 0 0 5.68-.494c1.584-.233 2.707-1.626 2.707-3.228V6.741c0-1.602-1.123-2.995-2.707-3.228A48.394 48.394 0 0 0 12 3c-2.392 0-4.744.175-7.043.513C3.373 3.746 2.25 5.14 2.25 6.741v6.018Z",
	);
	svg.appendChild(path);
	return svg;
}
