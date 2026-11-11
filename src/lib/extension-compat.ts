import type { Extension, ExtensionGroup } from "@/types";

/** The extension family an engine loads: Camoufox Firefox add-ons, Wayfern
 * Chromium extensions (`browser_compatibility` values). */
const ENGINE_EXTENSION_FAMILY: Record<string, string> = {
  camoufox: "firefox",
  wayfern: "chromium",
};

/**
 * Whether every extension of the group runs in the engine, the rule the
 * backend applies (`validate_group_compatibility`). An extension the list
 * does not know is left to the backend, as is an engine without extensions.
 */
export function extensionGroupFitsEngine(
  group: Pick<ExtensionGroup, "extension_ids">,
  extensionsById: ReadonlyMap<string, Pick<Extension, "browser_compatibility">>,
  engine: string,
): boolean {
  const family = ENGINE_EXTENSION_FAMILY[engine];
  if (!family) return true;
  return group.extension_ids.every((id) => {
    const extension = extensionsById.get(id);
    return !extension || extension.browser_compatibility.includes(family);
  });
}
