/** Display legacy system messages without renaming players or rewriting saved history. */
export function chronicleText(event) {
  const text = String(event.text ?? '');
  if (event.type === 'CITIZEN_JOINED') {
    return text.replace(/がアスターの市民になった。$/, 'が文明の市民になった。');
  }
  if (event.type === 'AMBITION_REACHED') {
    return text.replace(/^アスターは開拓期の共同目標を達成した。/, '私たちの文明は開拓期の共同目標を達成した。');
  }
  return text;
}
