type KeyboardLike = Pick<
  KeyboardEvent,
  "key" | "code" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey"
>;

/**
 * Physical keys (`KeyboardEvent.code`) the backend can parse.
 *
 * `global-hotkey`'s `parse_key` only accepts A-Z / 0-9 / F1-F12 / Arrow* /
 * Enter / Escape / Backspace / Tab / Space, so everything below stays inside
 * that whitelist. Anything else is rejected in the UI instead of failing on save.
 */
const NAMED_CODES: Record<string, string> = {
  Space: "Space",
  Enter: "Enter",
  NumpadEnter: "Enter",
  Escape: "Escape",
  Backspace: "Backspace",
  Tab: "Tab",
  ArrowUp: "ArrowUp",
  ArrowDown: "ArrowDown",
  ArrowLeft: "ArrowLeft",
  ArrowRight: "ArrowRight",
};

/** Modifier (and lock) keys never become the non-modifier part of a shortcut. */
const MODIFIER_CODES = new Set([
  "MetaLeft",
  "MetaRight",
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "ShiftLeft",
  "ShiftRight",
  "CapsLock",
  "NumLock",
  "ScrollLock",
  "Fn",
  "FnLock",
]);

export function toAccelerator(event: KeyboardLike): string | null {
  const modifiers: string[] = [];
  if (event.metaKey) modifiers.push("CmdOrCtrl");
  if (event.ctrlKey && !event.metaKey) modifiers.push("Ctrl");
  if (event.altKey) modifiers.push("Alt");
  if (event.shiftKey) modifiers.push("Shift");

  if (!modifiers.length) return null;
  if (MODIFIER_CODES.has(event.code)) return null;

  const key = normalizeKey(event);
  if (!key) return null;
  return [...modifiers, key].join("+");
}

/**
 * Map a physical key to an accelerator token.
 *
 * `code` is used instead of `key` on purpose: with Shift held, `event.key` for
 * Digit1 is "!" and for Space it is " ", both of which the backend rejects.
 * `code` is also layout/IME independent, so Cyrillic or CJK input methods still
 * record the key the user actually pressed.
 */
function normalizeKey(event: KeyboardLike): string | null {
  const code = event.code ?? "";
  if (!code) return null;
  const named = NAMED_CODES[code];
  if (named) return named;

  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return letter[1];

  const digit = /^(?:Digit|Numpad)([0-9])$/.exec(code);
  if (digit) return digit[1];

  const fn = /^F([1-9]|1[0-2])$/.exec(code);
  if (fn) return `F${fn[1]}`;

  // Symbols (Minus, Slash, NumpadAdd…) are not supported by the backend.
  return null;
}

export function keycaps(accelerator: string): string[] {
  const isMac = /Mac|iPhone|iPad/.test(navigator.userAgent);
  return accelerator.split("+").map((key) => {
    switch (key.toLowerCase()) {
      case "cmdorctrl":
      case "commandorcontrol":
        return isMac ? "⌘" : "Ctrl";
      case "ctrl":
      case "control":
        return isMac ? "⌃" : "Ctrl";
      case "alt":
      case "option":
        return isMac ? "⌥" : "Alt";
      case "shift":
        return isMac ? "⇧" : "Shift";
      case "meta":
      case "super":
        return isMac ? "⌘" : "Win";
      case "space":
        return "Space";
      case "arrowup":
        return "↑";
      case "arrowdown":
        return "↓";
      case "arrowleft":
        return "←";
      case "arrowright":
        return "→";
      default:
        return key.length === 1 ? key.toUpperCase() : key;
    }
  });
}
