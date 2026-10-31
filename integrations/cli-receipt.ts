type Scalar = string | number | boolean | null;

/** Session CLI receipts are flat. Accept older JSON releases during upgrades. */
export function parseScalarReceipt(text: string): Record<string, Scalar> {
  if (text.trimStart().startsWith("{")) {
    const value = JSON.parse(text);
    if (!value || typeof value !== "object" || Array.isArray(value)
      || Object.values(value).some(item => item !== null && typeof item === "object")) {
      throw new Error("Expected a scalar session receipt");
    }
    return value;
  }
  const result: Record<string, Scalar> = Object.create(null);
  for (const line of text.trimEnd().split("\n")) {
    const match = /^([\w][\w-]*):(.*)$/.exec(line);
    if (!match || Object.hasOwn(result, match[1])) throw new Error("Invalid session receipt");
    const [, key, raw] = match;
    const value = raw.trim();
    if (value.startsWith('"')) {
      if (!/^"(?:[^"\\]|\\[\\n"])*"$/.test(value)) throw new Error("Invalid LEAN string");
      result[key] = value.slice(1, -1).replace(/\\([\\n"])/g, (_, escape) => escape === "n" ? "\n" : escape);
    } else if (value === "T" || value === "F") {
      result[key] = value === "T";
    } else if (value === "_") {
      result[key] = null;
    } else if (/^-?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$/.test(value)) {
      result[key] = Number(value);
    } else {
      result[key] = value;
    }
  }
  return result;
}
