// DOM minimal : juste ce que les overlays utilisent (construction par API DOM,
// classList, style, querySelector par classe). Pas de dépendance ajoutée au projet.
'use strict';

class ClassList {
  constructor(el) { this.el = el; }
  add(...c)    { for (const x of c) this.el._classes.add(x); }
  remove(...c) { for (const x of c) this.el._classes.delete(x); }
  contains(c)  { return this.el._classes.has(c); }
  toggle(c, force) {
    const on = force === undefined ? !this.contains(c) : !!force;
    if (on) this.add(c); else this.remove(c);
    return on;
  }
  get value() { return [...this.el._classes].join(' '); }
}

class Style {
  constructor() { this._props = new Map(); }
  setProperty(k, v)  { this._props.set(k, String(v)); }
  removeProperty(k)  { this._props.delete(k); }
  getPropertyValue(k) { return this._props.get(k) ?? ''; }
}

let nodeSeq = 0;

class Element {
  constructor(tag) {
    this.tagName = String(tag).toUpperCase();
    this.uid = ++nodeSeq;
    this.children = [];
    this.parentNode = null;
    this._classes = new Set();
    this._text = '';
    this._html = null;
    this.style = new Style();
    this.classList = new ClassList(this);
    this._listeners = new Map();
  }

  addEventListener(type, fn) {
    if (!this._listeners.has(type)) this._listeners.set(type, []);
    this._listeners.get(type).push(fn);
  }

  /** Déclenche les écouteurs enregistrés, comme le ferait un clic ou une saisie. */
  dispatch(type, event = {}) {
    for (const fn of this._listeners.get(type) ?? []) fn({ target: this, ...event });
  }

  set className(v) { this._classes = new Set(String(v).split(/\s+/).filter(Boolean)); }
  get className()  { return [...this._classes].join(' '); }

  set textContent(v) { this._text = String(v); this.children = []; this._html = null; }
  get textContent()  {
    return this.children.length
      ? this.children.map((c) => c.textContent).join('')
      : this._text;
  }

  /**
   * `innerHTML` n'est pas analysé : la valeur posée est conservée telle quelle,
   * ce qui suffit à vérifier le balisage produit par les pages de configuration.
   *
   * En lecture, un nœud purement textuel renvoie son texte échappé comme le fait
   * un navigateur — `&`, `<` et `>` seulement, **pas** les guillemets. C'est
   * exactement ce comportement qui rend le motif `textContent` → `innerHTML`
   * impropre à l'échappement d'une valeur d'attribut.
   */
  set innerHTML(v) { this._html = String(v); this.children = []; this._text = ''; }
  get innerHTML() {
    if (this._html !== null) return this._html;
    if (this.children.length) return this.children.map((c) => c.innerHTML).join('');
    return this._text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  }

  get firstElementChild() { return this.children[0] ?? null; }

  appendChild(child) {
    if (child.parentNode) child.parentNode.removeChild(child);
    child.parentNode = this;
    this.children.push(child);
    // Le balisage posé par un `innerHTML =` précédent ne fait plus autorité dès
    // qu'on greffe des nœuds : `innerHTML` doit repartir des enfants réels.
    this._html = null;
    return child;
  }

  removeChild(child) {
    const i = this.children.indexOf(child);
    if (i >= 0) this.children.splice(i, 1);
    child.parentNode = null;
    return child;
  }

  remove() { if (this.parentNode) this.parentNode.removeChild(this); }

  replaceChildren(...nodes) {
    for (const c of this.children) c.parentNode = null;
    this.children = [];
    for (const n of nodes) this.appendChild(n);
  }

  /** Sélecteurs de classe uniquement (`.foo`) — c'est tout ce qu'utilise le code. */
  querySelector(sel) {
    const want = sel.replace(/^\./, '');
    for (const c of this.children) {
      if (c._classes.has(want)) return c;
      const deep = c.querySelector(sel);
      if (deep) return deep;
    }
    return null;
  }

  querySelectorAll(sel) {
    const want = sel.replace(/^\./, '');
    const out = [];
    for (const c of this.children) {
      if (c._classes.has(want)) out.push(c);
      out.push(...c.querySelectorAll(sel));
    }
    return out;
  }

  cloneNode() {
    const copy = new Element(this.tagName);
    copy._classes = new Set(this._classes);
    copy._text = this._text;
    for (const c of this.children) copy.appendChild(c.cloneNode(true));
    return copy;
  }

  getBoundingClientRect() {
    // Hauteur fictive proportionnelle au nombre de barres : suffit pour vérifier
    // que --goal-dock-h suit bien l'état visible/masqué.
    return { height: this.children.length * 60, width: 800, top: 0, left: 0 };
  }
}

function el(tag, className) {
  const e = new Element(tag);
  if (className) e.className = className;
  return e;
}

/** Reproduit le <template id="goalTpl"> du partiel. */
function buildGoalTemplate() {
  const tpl = el('template');
  tpl.content = el('div', '__content__');

  const goal = el('div', 'goal');
  const head = el('div', 'goal-head');
  head.appendChild(el('span', 'goal-title'));
  const nums = el('span', 'goal-numbers');
  nums.appendChild(el('span', 'goal-current'));
  nums.appendChild(el('span', 'goal-separator'));
  nums.appendChild(el('span', 'goal-target'));
  head.appendChild(nums);
  goal.appendChild(head);

  const track = el('div', 'goal-track');
  track.appendChild(el('div', 'goal-fill'));
  track.appendChild(el('div', 'goal-percent'));
  goal.appendChild(track);
  goal.appendChild(el('div', 'goal-warning goal-hide'));

  tpl.content.appendChild(goal);
  return tpl;
}

function makeDocument(ids = {}) {
  const registry = new Map(Object.entries(ids));
  return {
    _registry: registry,
    activeElement: null,
    getElementById: (id) => registry.get(id) ?? null,
    createElement: (tag) => el(tag),
    querySelectorAll: () => [],
  };
}

module.exports = { Element, el, buildGoalTemplate, makeDocument };
