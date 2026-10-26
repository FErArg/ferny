// Ferny UI mockups: a click on any part shows the name of the object that
// draws it in the application (Qt class, member or objectName, source).
// Esc or a click outside an object hides the label.
(function () {
  const tip = document.createElement("div");
  tip.id = "inspector";
  document.body.appendChild(tip);
  let current = null;

  function hide() {
    tip.style.display = "none";
    if (current) current.classList.remove("inspected");
    current = null;
  }

  function show(el, x, y) {
    if (current) current.classList.remove("inspected");
    current = el;
    el.classList.add("inspected");
    const name = el.dataset.name || "";
    const cls = el.dataset.obj;
    const src = el.dataset.src || "";
    const note = el.dataset.note || "";
    tip.innerHTML = "";
    const title = document.createElement("b");
    title.textContent = name || cls;
    tip.appendChild(title);
    const line = document.createElement("div");
    const code = document.createElement("code");
    code.textContent = cls;
    line.appendChild(code);
    tip.appendChild(line);
    if (note) {
      const n = document.createElement("div");
      n.textContent = note;
      tip.appendChild(n);
    }
    if (src) {
      const s = document.createElement("div");
      s.className = "src";
      s.textContent = src;
      tip.appendChild(s);
    }
    tip.style.display = "block";
    const w = tip.offsetWidth, h = tip.offsetHeight;
    tip.style.left = Math.min(x + 14, window.innerWidth - w - 8) + "px";
    tip.style.top = Math.min(y + 14, window.innerHeight - h - 8) + "px";
  }

  document.addEventListener("click", (event) => {
    const el = event.target.closest("[data-obj]");
    if (!el) { hide(); return; }
    event.preventDefault();
    event.stopPropagation();
    show(el, event.clientX, event.clientY);
  }, true);
  document.addEventListener("keydown", (event) => { if (event.key === "Escape") hide(); });
  window.addEventListener("scroll", hide, { passive: true });
})();
