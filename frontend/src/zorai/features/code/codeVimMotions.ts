export type VimPos = { line: number; col: number };
export type VimRange = { start: VimPos; end: VimPos; linewise: boolean };
export type VimCommand =
  | { kind: "motion"; count: number; motion: string }
  | { kind: "operator"; count: number; operator: "d" | "c" | "y" | ">" | "<"; motion?: string; textObject?: string; linewise?: boolean }
  | { kind: "action"; count: number; action: string };

const MOTIONS = new Set(["h", "j", "k", "l", "w", "W", "b", "B", "e", "E", "0", "^", "$", "G", "gg", "{", "}", "%"]);
const ACTIONS = new Set(["x", "X", "p", "P", "u", "i", "a", "I", "A", "o", "O", "v", "V", "J", "r", "~", "n", "N", "s", "S", "C", "D", "Y", ".", "zz"]);
const OBJECT_KEYS = new Set(["w", "W", "\"", "'", "`", "(", ")", "b", "[", "]", "{", "}", "B", "<", ">", "t", "p"]);

export function parseVimCommand(buffer: string): VimCommand | "pending" | "invalid" {
  const match = /^([1-9]\d*)?(.*)$/.exec(buffer);
  const count = match?.[1] ? Number(match[1]) : 1;
  const rest = match?.[2] ?? "";
  if (rest === "") return "pending";
  if (rest === "g" || rest === "z") return "pending";
  if (MOTIONS.has(rest)) return { kind: "motion", count, motion: rest };
  if (rest.startsWith("r")) return rest.length === 2 ? { kind: "action", count, action: rest } : "pending";
  if (ACTIONS.has(rest)) return { kind: "action", count, action: rest };
  const operator = rest[0] as "d" | "c" | "y" | ">" | "<";
  if (!["d", "c", "y", ">", "<"].includes(operator)) return "invalid";
  const tail = rest.slice(1);
  if (tail === "") return "pending";
  if (tail === operator) return { kind: "operator", count, operator, linewise: true };
  const motionMatch = /^([1-9]\d*)?(.*)$/.exec(tail);
  const innerCount = motionMatch?.[1] ? Number(motionMatch[1]) : 1;
  const motion = motionMatch?.[2] ?? "";
  if (motion === "" || motion === "g") return "pending";
  if (motion === "i" || motion === "a") return "pending";
  if ((motion[0] === "i" || motion[0] === "a") && motion.length === 2) return OBJECT_KEYS.has(motion[1]) ? { kind: "operator", count: count * innerCount, operator, textObject: motion } : "invalid";
  if (MOTIONS.has(motion)) return { kind: "operator", count: count * innerCount, operator, motion };
  return "invalid";
}

const isWord = (char: string | undefined) => char !== undefined && /[\w]/.test(char);
const isBlank = (char: string | undefined) => char === undefined || /\s/.test(char);
const charClass = (char: string | undefined, big: boolean) => (isBlank(char) ? 0 : big || isWord(char) ? 1 : 2);

function flat(lines: string[]) {
  const starts: number[] = [];
  let offset = 0;
  for (const line of lines) {
    starts.push(offset);
    offset += line.length + 1;
  }
  const text = lines.join("\n");
  const toPos = (index: number): VimPos => {
    let line = 0;
    while (line + 1 < starts.length && starts[line + 1] <= index) line += 1;
    return { line, col: index - starts[line] };
  };
  return { text, toIndex: (pos: VimPos) => starts[pos.line] + pos.col, toPos };
}

export function applyMotion(lines: string[], pos: VimPos, motion: string, count = 1): VimPos {
  const clampCol = (line: number, col: number) => Math.max(0, Math.min(col, Math.max(0, lines[line].length - 1)));
  const { text, toIndex, toPos } = flat(lines);
  let current = { ...pos };
  for (let step = 0; step < count; step += 1) {
    if (motion === "h") current = { line: current.line, col: Math.max(0, current.col - 1) };
    else if (motion === "l") current = { line: current.line, col: clampCol(current.line, current.col + 1) };
    else if (motion === "j") current = { line: Math.min(lines.length - 1, current.line + 1), col: 0 };
    else if (motion === "k") current = { line: Math.max(0, current.line - 1), col: 0 };
    else if (motion === "0") return { line: current.line, col: 0 };
    else if (motion === "^") return { line: current.line, col: Math.max(0, lines[current.line].search(/\S|$/)) };
    else if (motion === "$") return { line: Math.min(lines.length - 1, current.line + count - 1), col: clampCol(Math.min(lines.length - 1, current.line + count - 1), Infinity) };
    else if (motion === "gg") return { line: count > 1 ? Math.min(lines.length - 1, count - 1) : 0, col: Math.max(0, lines[count > 1 ? Math.min(lines.length - 1, count - 1) : 0].search(/\S|$/)) };
    else if (motion === "G") return { line: count > 1 ? Math.min(lines.length - 1, count - 1) : lines.length - 1, col: 0 };
    else if (motion === "w" || motion === "W") {
      const big = motion === "W";
      let index = toIndex(current);
      const start = charClass(text[index], big);
      while (index < text.length && start !== 0 && charClass(text[index], big) === start) index += 1;
      while (index < text.length && isBlank(text[index])) {
        if (text[index] === "\n" && text[index + 1] === "\n") { index += 1; break; }
        index += 1;
      }
      current = toPos(Math.min(index, text.length));
    } else if (motion === "b" || motion === "B") {
      const big = motion === "B";
      let index = toIndex(current) - 1;
      while (index > 0 && isBlank(text[index])) index -= 1;
      const cls = charClass(text[index], big);
      while (index > 0 && charClass(text[index - 1], big) === cls && cls !== 0) index -= 1;
      current = toPos(Math.max(0, index));
    } else if (motion === "e" || motion === "E") {
      const big = motion === "E";
      let index = toIndex(current) + 1;
      while (index < text.length && isBlank(text[index])) index += 1;
      const cls = charClass(text[index], big);
      while (index + 1 < text.length && charClass(text[index + 1], big) === cls) index += 1;
      current = toPos(Math.min(index, text.length - 1));
    } else if (motion === "}" || motion === "{") {
      let line = current.line + (motion === "}" ? 1 : -1);
      while (line > 0 && line < lines.length - 1 && lines[line].trim() !== "") line += motion === "}" ? 1 : -1;
      current = { line: Math.max(0, Math.min(lines.length - 1, line)), col: 0 };
    } else if (motion === "%") {
      const pairs: Record<string, string> = { "(": ")", "[": "]", "{": "}", ")": "(", "]": "[", "}": "{" };
      let index = toIndex(current);
      while (index < text.length && !pairs[text[index]] && text[index] !== "\n") index += 1;
      const open = text[index];
      if (!pairs[open]) return current;
      const forward = "([{".includes(open);
      let depth = 0;
      for (let cursor = index; cursor >= 0 && cursor < text.length; cursor += forward ? 1 : -1) {
        if (text[cursor] === open) depth += 1;
        else if (text[cursor] === pairs[open]) depth -= 1;
        if (depth === 0) return toPos(cursor);
      }
      return current;
    }
  }
  return current;
}

export function textObjectRange(lines: string[], pos: VimPos, object: string): VimRange | null {
  const { text, toIndex, toPos } = flat(lines);
  const index = toIndex(pos);
  const around = object[0] === "a";
  const key = object[1];
  if (key === "w" || key === "W") {
    const big = key === "W";
    const cls = charClass(text[index], big);
    let start = index;
    let end = index;
    while (start > 0 && charClass(text[start - 1], big) === cls && text[start - 1] !== "\n") start -= 1;
    while (end + 1 < text.length && charClass(text[end + 1], big) === cls && text[end + 1] !== "\n") end += 1;
    if (around) while (end + 1 < text.length && text[end + 1] === " ") end += 1;
    return { start: toPos(start), end: toPos(end + 1), linewise: false };
  }
  if (key === "p") {
    let first = pos.line;
    let last = pos.line;
    while (first > 0 && lines[first - 1].trim() !== "") first -= 1;
    while (last + 1 < lines.length && lines[last + 1].trim() !== "") last += 1;
    if (around && last + 1 < lines.length) last += 1;
    return { start: { line: first, col: 0 }, end: { line: last, col: lines[last].length }, linewise: true };
  }
  const pairs: Record<string, [string, string]> = { "(": ["(", ")"], ")": ["(", ")"], b: ["(", ")"], "[": ["[", "]"], "]": ["[", "]"], "{": ["{", "}"], "}": ["{", "}"], B: ["{", "}"], "<": ["<", ">"], ">": ["<", ">"] };
  if (pairs[key]) {
    const [open, close] = pairs[key];
    let depth = 0;
    let start = -1;
    for (let cursor = index; cursor >= 0; cursor -= 1) {
      if (text[cursor] === close && cursor !== index) depth += 1;
      else if (text[cursor] === open) {
        if (depth === 0) { start = cursor; break; }
        depth -= 1;
      }
    }
    if (start < 0) return null;
    depth = 0;
    for (let cursor = start + 1; cursor < text.length; cursor += 1) {
      if (text[cursor] === open) depth += 1;
      else if (text[cursor] === close) {
        if (depth === 0) return around ? { start: toPos(start), end: toPos(cursor + 1), linewise: false } : { start: toPos(start + 1), end: toPos(cursor), linewise: false };
        depth -= 1;
      }
    }
    return null;
  }
  if (key === "\"" || key === "'" || key === "`") {
    const line = lines[pos.line];
    const quotes = [...line].map((char, col) => (char === key ? col : -1)).filter((col) => col >= 0);
    for (let pair = 0; pair + 1 < quotes.length; pair += 2) {
      if (pos.col >= quotes[pair] && pos.col <= quotes[pair + 1]) {
        return around
          ? { start: { line: pos.line, col: quotes[pair] }, end: { line: pos.line, col: quotes[pair + 1] + 1 }, linewise: false }
          : { start: { line: pos.line, col: quotes[pair] + 1 }, end: { line: pos.line, col: quotes[pair + 1] }, linewise: false };
      }
    }
  }
  return null;
}

export function operatorRange(lines: string[], pos: VimPos, command: Extract<VimCommand, { kind: "operator" }>): VimRange | null {
  if (command.linewise) {
    const last = Math.min(lines.length - 1, pos.line + command.count - 1);
    return { start: { line: pos.line, col: 0 }, end: { line: last, col: lines[last].length }, linewise: true };
  }
  if (command.textObject) return textObjectRange(lines, pos, command.textObject);
  if (!command.motion) return null;
  const motion = command.operator === "c" && command.motion === "w" ? "e" : command.motion === "cw" ? "e" : command.motion;
  const target = applyMotion(lines, pos, motion, command.count);
  const linewise = ["j", "k", "G", "gg", "{", "}"].includes(command.motion);
  const [from, to] = target.line < pos.line || (target.line === pos.line && target.col < pos.col) ? [target, pos] : [pos, target];
  if (linewise) return { start: { line: from.line, col: 0 }, end: { line: to.line, col: lines[to.line].length }, linewise: true };
  const inclusive = ["e", "E", "$", "%"].includes(motion) || (command.operator === "c" && command.motion === "w");
  return { start: from, end: { line: to.line, col: to.col + (inclusive ? 1 : 0) }, linewise: false };
}

export function substituteLine(line: string, command: string): string | null {
  const match = /^s\/((?:\\.|[^/])*)\/((?:\\.|[^/])*)\/?([gi]*)$/.exec(command);
  if (!match) return null;
  try {
    return line.replace(new RegExp(match[1], match[3].includes("g") ? `g${match[3].includes("i") ? "i" : ""}` : match[3].includes("i") ? "i" : ""), match[2].replace(/\\\//g, "/"));
  } catch {
    return null;
  }
}
