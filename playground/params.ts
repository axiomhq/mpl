import { MplParamType, MplSystemParam } from "@axiomhq/mpl-codemirror";

/**
 * Formats a raw user input into the MPL atomic literal its type expects.
*/
export function formatValue(type: MplParamType, raw: string): string {
  const v = raw.trim();
  switch (type) {
    case "string":
      return /^".*"$/s.test(v) ? v : JSON.stringify(v);
    case "Dataset":
      return v.startsWith("`") ? v : `\`${v}\``;
    case "Regex":
      return v.startsWith("#/") ? v : `#/${v}/`;
    case "bool":
    case "int":
    case "float":
    case "Duration":
      return v;
  }
}

/**
* Replaces the declared params with the user provided values.
*/
export function substituteParams(
  doc: string,
  decls: MplSystemParam[],
  values: Record<string, string>,
): string {
  return doc
    .split("\n")
    .map((line) => {
      // Skip declaration lines
      if (/^\s*param\s+\$/.test(line)) return line;
      let out = line;
      for (const param of decls) {
        const raw = values[param.name];
        if (!raw || param.optional) continue;

        const ref = new RegExp(`\\$${param.name}\\b`, "g");
        out = out.replace(ref, formatValue(param.type, raw));
      }
      return out;
    })
    .join("\n");
}
