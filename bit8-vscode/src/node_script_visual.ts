/** Conservative presentation hint, not a Lua parser or evaluator. */
export function staticNodeSprite(source: string): string | undefined {
  const text = maskCommentsAndStrings(source);
  if (text === undefined || /\bdraw\s*=/.test(text)) return undefined;
  const declarations = [...text.matchAll(/\b(?:func|function)\s+draw\s*\(\s*\)/g)];
  if (declarations.length !== 1) return undefined;
  const declaration = declarations[0];
  const before = text.slice(0, declaration.index);
  // Only a top-level declaration is eligible. Lua block openers are enough
  // here: the eligible draw body itself must consist of one exact statement.
  let depth = 0;
  for (const token of before.match(/\b(?:func|function|if|do|repeat|end|until)\b/g) ?? []) {
    depth += token === "end" || token === "until" ? -1 : 1;
    if (depth < 0) return undefined;
  }
  if (depth !== 0 || /\blocal\s*$/.test(before)) return undefined;
  const tail = text.slice(declaration.index! + declaration[0].length);
  const body = /^\s*(?:spr|sprite)\s*\(\s*([A-Z]+[1-9][0-9]*)\s*,\s*self\s*\.\s*x\s*,\s*self\s*\.\s*y\s*\)\s*;?\s*end\b/.exec(tail);
  return body?.[1];
}

// Mask strings too, so examples inside quoted text cannot become declarations.
// Unterminated comments/strings produce no hint. Preserve newlines/offsets.
function maskCommentsAndStrings(source: string): string | undefined {
  let result = "", index = 0;
  const masked = (text: string, string: boolean): string => text.replace(/[^\r\n]/g, string ? "#" : " ");
  while (index < source.length) {
    const comment = source.startsWith("--", index);
    const start = index + (comment ? 2 : 0);
    const long = /^\[(=*)\[/.exec(source.slice(start));
    if (long) {
      const end = source.indexOf(`]${long[1]}]`, start + long[0].length);
      if (end < 0) return undefined;
      const next = end + long[1].length + 2;
      result += masked(source.slice(index, next), !comment); index = next;
    } else if (comment) {
      const end = source.indexOf("\n", index);
      const next = end < 0 ? source.length : end;
      result += masked(source.slice(index, next), false); index = next;
    } else if (source[index] === '"' || source[index] === "'") {
      const quote = source[index]; let end = index + 1;
      while (end < source.length && source[end] !== quote) {
        if (source[end] === "\\") end++;
        end++;
      }
      if (end >= source.length) return undefined;
      result += masked(source.slice(index, end + 1), true); index = end + 1;
    } else { result += source[index++]; }
  }
  return result;
}
