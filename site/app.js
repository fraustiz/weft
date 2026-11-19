// Weft landing: download links from the latest GitHub release, and the
// transfer scene: the copied text runs along the lit thread from the phone to
// the Mac, once, on request, with pause, resume and replay.
(() => {
  "use strict";

  /* Downloads ------------------------------------------------------------ */
  const ASSETS = { android: /\.apk$/i, mac: /-mac\.zip$/i, windows: /-windows\.exe$/i, ios: /-ios\.ipa$/i };
  const LABELS = {
    android: "Télécharger pour Android",
    mac: "Télécharger pour Mac",
    windows: "Télécharger pour Windows",
    ios: "Télécharger pour iPhone",
  };

  function detectPlatform() {
    const ua = navigator.userAgent || "";
    if (/Android/i.test(ua)) return "android";
    if (/iPhone|iPad|iPod/i.test(ua) || (/Macintosh/.test(ua) && navigator.maxTouchPoints > 1)) return "ios";
    if (/Macintosh|Mac OS X/i.test(ua)) return "mac";
    if (/Windows/i.test(ua)) return "windows";
    return null;
  }

  async function wireDownloads() {
    let release;
    try {
      const res = await fetch("https://api.github.com/repos/fraustiz/weft/releases/latest", {
        headers: { Accept: "application/vnd.github+json" },
      });
      if (!res.ok) return;
      release = await res.json();
    } catch {
      return; // links keep pointing at the latest release page
    }
    const version = String(release.tag_name || "").replace(/^app-v/, "");
    if (version) document.querySelectorAll("[data-release-version]").forEach((el) => { el.textContent = version; });
    const urls = {};
    for (const asset of release.assets || []) {
      for (const [key, re] of Object.entries(ASSETS)) if (re.test(asset.name)) urls[key] = asset.browser_download_url;
    }
    document.querySelectorAll("[data-dl]").forEach((a) => { if (urls[a.dataset.dl]) a.href = urls[a.dataset.dl]; });
    const platform = detectPlatform();
    const primary = document.querySelector('[data-dl="primary"]');
    if (primary && platform && urls[platform]) {
      primary.href = urls[platform];
      primary.textContent = LABELS[platform];
    }
  }
  wireDownloads();

  /* Transfer scene -------------------------------------------------------- */
  // Four real Weft transfers loop between the phone and the Mac. The loop
  // pauses on hover, on focus, off screen, in a hidden tab and on the Pause
  // button; with reduced motion nothing plays and the examples switch in place.
  const scene = document.getElementById("scene");
  if (!scene || !("animate" in Element.prototype)) return;

  const q = (s) => scene.querySelector(s);
  const controls = q(".stage-controls");
  const pauseBtn = q(".ctl-pause");
  const pauseLabel = q(".ctl-label");
  const exButtons = [...scene.querySelectorAll("[data-ex]")];
  const caption = q(".stage-status");
  const clip = { phone: q(".clip--phone"), mac: q(".clip--mac") };
  const press = { phone: q(".press--phone"), mac: q(".press--mac") };
  const flash = { phone: q(".flash--phone"), mac: q(".flash--mac") };
  const marker = { phone: q(".marker--phone"), mac: q(".marker--mac") };
  const fly = q(".clip--fly");
  const halo = q(".thread-halo");
  const base = q(".thread-base");
  const lit = q(".thread-lit");
  const grad = q("#thread-grad");
  const reduce = matchMedia("(prefers-reduced-motion: reduce)");
  const canOffset = CSS.supports("offset-path", 'path("M0 0 L1 1")');

  const EXAMPLES = [
    { id: "text", from: "phone", press: "phone", kind: "Texte",
      phone: ["Copier", "sur le téléphone"], mac: ["Coller", "sur le Mac, ⌘V"],
      caption: "Un texte copié sur le téléphone, collé sur le Mac." },
    { id: "image", from: "mac", press: null, kind: "Image",
      mac: ["Copier", "sur le Mac, ⌘C"], phone: ["Accepter", "sur le téléphone"],
      caption: "Une image copiée sur le Mac est proposée au téléphone." },
    { id: "file", from: "mac", press: "mac", kind: "Fichier",
      mac: ["Envoyer", "un fichier, ou le déposer"], phone: ["Recevoir", "dans Téléchargements"],
      caption: "Un fichier envoyé du Mac arrive dans les Téléchargements du téléphone." },
    { id: "link", from: "mac", press: "mac", kind: "Lien",
      mac: ["Partager", "un lien"], phone: ["Ouvrir", "sur le téléphone"],
      caption: "Un lien envoyé du Mac s’ouvre sur le téléphone." },
  ];
  const DURATION = 5600;
  const EASE = "cubic-bezier(0.65, 0, 0.35, 1)";

  let index = 0;
  let anims = [];
  let geo = null;
  const hold = { user: false, hover: false, focus: false, offscreen: false, hidden: document.hidden };

  function layout() {
    const s = scene.getBoundingClientRect();
    const a = clip.phone.getBoundingClientRect();
    const b = clip.mac.getBoundingClientRect();
    const x1 = a.left - s.left + a.width / 2, y1 = a.top - s.top + a.height / 2;
    const x2 = b.left - s.left + b.width / 2, y2 = b.top - s.top + b.height / 2;
    const dx = x2 - x1, dy = y2 - y1;
    const dist = Math.hypot(dx, dy) || 1;
    // bulge along the normal that points up (or right when the run is vertical)
    let nx = -dy / dist, ny = dx / dist;
    if (ny > 0 || (Math.abs(ny) < 0.3 && nx < 0)) { nx = -nx; ny = -ny; }
    const k = Math.max(50, dist * 0.32);
    const d = `M ${x1} ${y1} C ${x1 + dx * 0.25 + nx * k} ${y1 + dy * 0.25 + ny * k}, ${x1 + dx * 0.75 + nx * k} ${y1 + dy * 0.75 + ny * k}, ${x2} ${y2}`;
    [halo, base, lit].forEach((p) => p.setAttribute("d", d));
    const len = lit.getTotalLength();
    lit.style.strokeDasharray = `${len} ${len}`;
    geo = { d, len, x1, y1, x2, y2 };
    if (canOffset) {
      fly.style.offsetPath = `path("${d}")`;
      fly.style.offsetRotate = "0deg";
      fly.style.offsetAnchor = "50% 50%";
    }
  }

  function fill(target, ex) {
    target.querySelector(".clip-k").textContent = ex.kind;
    const body = target.querySelector(".clip-body");
    body.replaceChildren(document.getElementById(`body-${ex.id}`).content.cloneNode(true));
  }

  function setMarker(side, [verb, how]) {
    marker[side].querySelector(".marker-verb").textContent = verb;
    marker[side].querySelector(".marker-how").textContent = how;
  }

  function stage(ex) {
    scene.dataset.example = ex.id;
    fill(clip.phone, ex); fill(clip.mac, ex); fill(fly, ex);
    fly.querySelector(".clip-k").textContent = "Chiffré";
    fly.style.width = `${clip.phone.getBoundingClientRect().width}px`;
    setMarker("phone", ex.phone); setMarker("mac", ex.mac);
    caption.textContent = ex.caption;
    exButtons.forEach((b) => b.setAttribute("aria-pressed", String(b.dataset.ex === ex.id)));
    const forward = ex.from === "phone";
    grad.setAttribute("x1", forward ? geo.x1 : geo.x2); grad.setAttribute("y1", forward ? geo.y1 : geo.y2);
    grad.setAttribute("x2", forward ? geo.x2 : geo.x1); grad.setAttribute("y2", forward ? geo.y2 : geo.y1);
  }

  function frames(ex) {
    const from = ex.from, to = from === "phone" ? "mac" : "phone";
    const forward = from === "phone";
    const still = reduce.matches;
    const list = [
      [clip[from], [
        { opacity: 0, transform: "translateY(10px) scale(0.96)", offset: 0 },
        { opacity: 0, transform: "translateY(10px) scale(0.96)", offset: 0.08 },
        { opacity: 1, transform: "none", offset: 0.16 },
        { opacity: 1, transform: "none", offset: 1 },
      ]],
      [clip[to], [
        { opacity: 0, transform: "scale(0.96)", offset: 0 },
        { opacity: 0, transform: "scale(0.96)", offset: 0.66 },
        { opacity: 1, transform: "none", offset: 0.74 },
        { opacity: 1, transform: "none", offset: 1 },
      ]],
      [lit, [
        { strokeDashoffset: forward ? geo.len : -geo.len, offset: 0 },
        { strokeDashoffset: forward ? geo.len : -geo.len, offset: 0.18, easing: EASE },
        { strokeDashoffset: 0, offset: 0.66 },
        { strokeDashoffset: 0, offset: 1 },
      ]],
      [halo, [
        { opacity: 0.12, offset: 0 }, { opacity: 0.12, offset: 0.18 },
        { opacity: 0.75, offset: 0.66 }, { opacity: 0.5, offset: 1 },
      ]],
      [flash[to], [
        { opacity: 0, offset: 0 }, { opacity: 0, offset: 0.66 },
        { opacity: 1, offset: 0.72 }, { opacity: 0.8, offset: 0.82 },
        { opacity: 0, offset: 0.95 }, { opacity: 0, offset: 1 },
      ]],
    ];
    if (ex.press) {
      list.push([press[ex.press], [
        { opacity: 0, transform: "scale(1)", offset: 0 },
        { opacity: 1, transform: "scale(1.04)", offset: 0.06 },
        { opacity: 1, transform: "scale(1)", offset: 0.14 },
        { opacity: 1, transform: "scale(1)", offset: 1 },
      ]]);
    } else {
      list.push([flash[from], [
        { opacity: 0, offset: 0 }, { opacity: 0.9, offset: 0.06 },
        { opacity: 0, offset: 0.2 }, { opacity: 0, offset: 1 },
      ]]);
    }
    const [d0, d1] = forward ? ["0%", "100%"] : ["100%", "0%"];
    const p0 = forward ? [geo.x1, geo.y1] : [geo.x2, geo.y2];
    const p1 = forward ? [geo.x2, geo.y2] : [geo.x1, geo.y1];
    if (still) {
      fly.style.offsetPath = "none";
      list.push([fly, [{ opacity: 0 }, { opacity: 0 }]]);
    } else if (canOffset) {
      list.push([fly, [
        { opacity: 0, offsetDistance: d0, transform: "scale(0.9)", offset: 0 },
        { opacity: 0, offsetDistance: d0, transform: "scale(0.9)", offset: 0.16 },
        { opacity: 1, offsetDistance: d0, transform: "scale(1)", offset: 0.2, easing: EASE },
        { opacity: 1, offsetDistance: d1, transform: "scale(1)", offset: 0.66 },
        { opacity: 0, offsetDistance: d1, transform: "scale(0.94)", offset: 0.72 },
        { opacity: 0, offsetDistance: d1, offset: 1 },
      ]]);
    } else {
      const w = fly.offsetWidth, h = fly.offsetHeight;
      const at = ([x, y]) => `translate(${x - w / 2}px, ${y - h / 2}px)`;
      list.push([fly, [
        { opacity: 0, transform: at(p0), offset: 0 },
        { opacity: 0, transform: at(p0), offset: 0.16 },
        { opacity: 1, transform: at(p0), offset: 0.2, easing: EASE },
        { opacity: 1, transform: at(p1), offset: 0.66 },
        { opacity: 0, transform: at(p1), offset: 0.72 },
        { opacity: 0, transform: at(p1), offset: 1 },
      ]]);
    }
    return list;
  }

  function stop() { anims.forEach((a) => a.cancel()); anims = []; }

  function play(i) {
    stop();
    index = i;
    layout();
    const ex = EXAMPLES[index];
    stage(ex);
    anims = frames(ex).map(([el, kf]) => el.animate(kf, { duration: DURATION, fill: "both", easing: "linear" }));
    anims[0].onfinish = () => { stop(); play((index + 1) % EXAMPLES.length); };
  }

  function show(i) { // the example at rest, nothing moving
    stop();
    index = i;
    layout();
    stage(EXAMPLES[index]);
  }

  const held = () => reduce.matches || Object.values(hold).some(Boolean);

  function update() {
    if (held()) { anims.forEach((a) => a.pause()); return; }
    if (anims.length) anims.forEach((a) => a.play());
    else play(index);
  }

  controls.hidden = false;
  if (reduce.matches) pauseBtn.hidden = true;
  show(0);
  document.fonts && document.fonts.ready.then(() => { if (!anims.length) show(index); });

  pauseBtn.addEventListener("click", () => {
    hold.user = !hold.user;
    pauseBtn.setAttribute("aria-pressed", String(hold.user));
    pauseLabel.textContent = hold.user ? "Lecture" : "Pause";
    update();
  });
  exButtons.forEach((b) => b.addEventListener("click", () => {
    const k = EXAMPLES.findIndex((ex) => ex.id === b.dataset.ex);
    if (held()) show(k); else play(k);
  }));
  // hovering the devices pauses the loop; the controls stay free to click
  scene.querySelectorAll(".device").forEach((dev) => {
    dev.addEventListener("pointerenter", (e) => { if (e.pointerType === "mouse") { hold.hover = true; update(); } });
    dev.addEventListener("pointerleave", () => { hold.hover = false; update(); });
  });
  // keyboard focus in the scene pauses it; a mouse click does not
  scene.addEventListener("focusin", (e) => { hold.focus = e.target.matches(":focus-visible"); update(); });
  scene.addEventListener("focusout", (e) => { if (!scene.contains(e.relatedTarget)) { hold.focus = false; update(); } });
  document.addEventListener("visibilitychange", () => { hold.hidden = document.hidden; update(); });
  reduce.addEventListener("change", () => { pauseBtn.hidden = reduce.matches; if (reduce.matches) show(index); update(); });
  if ("IntersectionObserver" in window) {
    new IntersectionObserver(([entry]) => { hold.offscreen = !entry.isIntersecting; update(); }, { threshold: 0.2 }).observe(scene);
  } else {
    update();
  }
  if ("ResizeObserver" in window) {
    let last = scene.offsetWidth;
    new ResizeObserver(() => {
      if (scene.offsetWidth === last) return;
      last = scene.offsetWidth;
      if (held()) show(index); else play(index);
    }).observe(scene);
  }
})();
