export function pluginUiSummary(plugin: Pick<ZoraiPluginUiManifest, "panels" | "commands">): string {
  const studios = plugin.panels.filter((panel) => panel.kind === "web").length;
  const views = plugin.panels.length - studios;
  return [
    studios ? `${studios} interactive panel${studios === 1 ? "" : "s"}` : "",
    views ? `${views} view${views === 1 ? "" : "s"}` : "",
    plugin.commands.length ? `${plugin.commands.length} command${plugin.commands.length === 1 ? "" : "s"}` : "",
  ].filter(Boolean).join(" · ");
}
