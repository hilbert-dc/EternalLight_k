export function isComposerSubmit(event: KeyboardEvent, shortcut: 'enter' | 'modifierEnter') {
  return event.key === 'Enter' && !event.isComposing && event.keyCode !== 229 && !event.repeat
    && !event.shiftKey && !event.altKey && (shortcut === 'enter' || event.metaKey || event.ctrlKey);
}
