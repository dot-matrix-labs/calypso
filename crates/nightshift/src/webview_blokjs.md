# BlokJS API Reference

> **Agents editing `webview_index.html`: read this file first.**
>
> `webview_index.html` uses [@maleta/blokjs](https://maleta.github.io/blokjs/docs/) as its
> reactive UI framework. This document is the authoritative quick-reference for the patterns
> used in that file. The full docs live at https://maleta.github.io/blokjs/docs/.

---

## `blok.mount(target, options)`

Mounts the root app. Returns `{ destroy() }`.

- `target` — CSS selector string or `HTMLElement`.
- `options` — plain object; all valid top-level keys are listed below.

### Valid top-level config keys

| Key | Description |
|-----|-------------|
| `state` | Reactive state object |
| `computed` | Derived values (getter functions, read-only) |
| `methods` | Functions callable from the view and other methods |
| `mount()` | Lifecycle hook — called after DOM insertion via `queueMicrotask` |
| `unmount()` | Lifecycle hook — called before element removal |
| `watch` | Reactions to state changes |
| `view($)` | **Required.** Function returning the plain-object view tree |

> **`mount` and `unmount` must be top-level keys — not inside `methods: {}`.**
> Placing them inside `methods` makes them plain methods; blokjs will never invoke
> them as lifecycle hooks.

---

## Reserved State Names

The following names **cannot** be used as `state` keys — blokjs will silently block
any attempt to set them and the property will stay `undefined`:

```
store  loading  error  refs  el  route  emit  navigate
```

`loading` and `error` are used by blokjs for automatic async-method tracking
(`this.loading.<methodName>` / `this.error.<methodName>`).

---

## State

```js
state: {
  count: 0,
  items: [],
  user: null,
}
```

- Read/write in methods/computed/watch/lifecycle as `this.<key>`.
- Read in view as `$.<key>`.
- Array mutations are reactive: `push`, `pop`, `shift`, `unshift`, `splice`, `sort`, `reverse`.

---

## Computed

```js
computed: {
  fullName() { return this.first + ' ' + this.last },
  filtered() { return this.items.filter(i => i.active) },
}
```

- Re-evaluated on every read (not cached).
- Read-only: `this.<key>` in methods; `$.<key>` in view.

---

## Methods

```js
methods: {
  inc() { this.count++ },
  async loadData() {
    const res = await fetch('/api/data');
    this.data = await res.json();
  },
}
```

Async methods are automatically tracked:
- `this.loading.<methodName>` — `true` while running.
- `this.error.<methodName>` — `Error` or `null` after completion.

---

## Lifecycle Hooks

```js
// Top-level — NOT inside methods: {}
mount() {
  // Called after DOM insertion via queueMicrotask.
  // this.refs and this.el are available here.
  this.loadData();
},

unmount() {
  // Called before element removal. Use for cleanup.
  clearInterval(this._timer);
},
```

---

## Watch

```js
watch: {
  search(newVal, oldVal) { this.filter(); },
  count(newVal, oldVal) { console.log(oldVal, '→', newVal); },
}
```

---

## View DSL

The `view` function receives `$` (a reactive reference proxy) and returns a plain
object describing the DOM.

### Text and children

```js
{ div: { text: $.count } }          // reactive text
{ div: 'Hello' }                    // shorthand static text
{ div: { html: $.rawHtml } }        // innerHTML (trusted content only)
{ div: { children: [{ p: 'a' }, { p: 'b' }] } }
```

### Conditionals

```js
{ when: $.isVisible,     children: [{ p: 'shown when truthy' }] }
{ when: $.not.isVisible, children: [{ p: 'shown when falsy'  }] }
```

### Loops

```js
{ each: $.items, as: 'item', key: 'id', children: [
  { div: { text: $.item.name } }
]}
```

`as` defaults to `'item'`; `key` is optional but recommended.

### Two-way binding

```js
{ input: { type: 'text',     model: $.search  } }
{ input: { type: 'checkbox', model: $.agree   } }
{ select: { model: $.chosen, children: [...] } }
{ textarea: { model: $.notes } }
```

### Classes

```js
{ div: { class: 'static-class' } }
{ div: { class: $.dynamicClass } }
{ div: { class: { active: $.isActive, disabled: $.isOff } } }
{ div: { class: ['base', { highlight: $.isOn }] } }
```

### Styles

```js
{ div: { style: 'color: red' } }
{ div: { style: { color: 'red', fontSize: '16px' } } }
{ div: { style: { backgroundColor: $.bgColor } } }
```

### Dynamic attributes

```js
{ img: { bind: { src: $.imageUrl, alt: $.title } } }
{ a:   { bind: { href: $.link }, link: true, text: 'Go' } }
```

### Events

```js
{ button: { click: 'methodName' } }                       // Event object passed
{ button: { click: 'remove(item)' } }                     // resolved path arg
{ button: { click: "select('literal')" } }                // string literal arg
{ button: { click: 'update(item, true, 42)' } }           // multiple args
{ button: { click: { handler: 'onClick', stop: true } } } // stopPropagation
{ form:   { submit: 'handleSubmit' } }                    // preventDefault auto-applied
{ button: { click: 'flag = true' } }                      // inline state assignment
```

Supported events: `click`, `dblclick`, `submit`, `input`, `change`, `focus`, `blur`,
`keydown`, `keyup`, `keypress`, `mousedown`, `mouseup`, `mousemove`, `mouseenter`,
`mouseleave`, `scroll`, `resize`, `dragstart`, `dragend`, `dragover`, `dragleave`,
`drop`, `touchstart`, `touchend`, `touchmove`.

### Refs

```js
{ input: { ref: 'nameInput' } }
// In mount() or methods:
this.refs.nameInput.focus();
```

### Components

```js
{ MyComponent: { propA: $.value, propB: 'static' } }
```

Child emits to parent:
```js
// child
methods: { remove() { this.emit('remove', this.item) } }
// parent view
{ MyComponent: { item: $.item, on_remove: 'handleRemove' } }
```

---

## `this` Context

Available inside `methods`, `computed`, `watch`, `mount`, `unmount`:

| Property | Description |
|----------|-------------|
| `this.<stateKey>` | Read/write reactive state |
| `this.<computedKey>` | Read computed value (read-only) |
| `this.store.<name>` | Access a named store |
| `this.refs.<name>` | DOM element ref (available after `mount`) |
| `this.el` | Component root element |
| `this.loading.<method>` | `true` while async method is running |
| `this.error.<method>` | `Error` or `null` after async method |
| `this.emit(event, data)` | Emit event to parent component |
| `this.navigate(path\|num)` | Router navigation |

---

## Stores

```js
blok.store('auth', {
  state: { user: null },
  computed: { isLoggedIn() { return this.user !== null } },
  methods: {
    async login(email, pw) { this.user = await fetch(...).then(r => r.json()); },
    logout() { this.user = null },
  },
});

// In any component view:
{ when: $.store.auth.isLoggedIn, children: [...] }
{ when: $.store.auth.loading.login, children: [{ p: 'Signing in…' }] }

// In methods:
this.store.auth.login(email, pw);
```

---

## Reactivity Notes

- Proxy-based: property reads inside effects (computed, watch, view) create reactive bindings automatically.
- Multiple synchronous state changes are batched and flush in one DOM update via microtask.
- Array mutations that trigger updates: `push`, `pop`, `shift`, `unshift`, `splice`, `sort`, `reverse`.

---

## Browser Requirements

ES2020+. Requires `Proxy`, `WeakMap`/`WeakSet`/`Symbol`, `queueMicrotask`, `globalThis`.
IE is not supported.
