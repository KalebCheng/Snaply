type KeyboardLike = Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">;

export function toAccelerator(event: KeyboardLike): string | null {
  const modifiers: string[] = [];
  if (event.metaKey) modifiers.push("CmdOrCtrl");
  if (event.ctrlKey && !event.metaKey) modifiers.push("Ctrl");
  if (event.altKey) modifiers.push("Alt");
  if (event.shiftKey) modifiers.push("Shift");

  const key = normalizeKey(event.key);
  if (!key || ["Meta", "Control", "Alt", "Shift"].includes(event.key) || modifiers.length === 0) {
    return null;
  }
  return [...modifiers, key].join("+");
}

function normalizeKey(key: string): string | null {
  if (key.length === 1) return /[a-z]/i.test(key) ? key.toUpperCase() : key;
  const namedKeys: Record<string, string> = {
    " ": "Space",
    ArrowUp: "ArrowUp",
    ArrowDown: "ArrowDown",
    ArrowLeft: "ArrowLeft",
    ArrowRight: "ArrowRight",
    Escape: "Escape",
    Enter: "Enter",
    Backspace: "Backspace",
    Tab: "Tab",
  };
  if (namedKeys[key]) return namedKeys[key];
  if (/^F\d{1,2}$/.test(key)) return key;
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
      default:
        return key.length === 1 ? key.toUpperCase() : key;
    }
  });
}
