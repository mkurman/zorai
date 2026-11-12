import { monogram } from "./pluginPrefsStore";

export function PluginIcon({ plugin, size = 28 }: { plugin: Pick<ZoraiPluginUiManifest, "name" | "icon">; size?: number }) {
  const style = { width: size, height: size, fontSize: Math.round(size * 0.42) };
  if (plugin.icon?.kind === "image") return <img className="zorai-plugin-icon" src={plugin.icon.url} alt="" style={style} />;
  if (plugin.icon?.kind === "glyph") return <span className="zorai-plugin-icon zorai-plugin-icon--glyph" style={{ ...style, fontSize: Math.round(size * 0.6) }} aria-hidden="true">{plugin.icon.text}</span>;
  const { letters, hue } = monogram(plugin.name);
  return <span className="zorai-plugin-icon zorai-plugin-icon--monogram" style={{ ...style, background: `hsl(${hue} 45% 38%)` }} aria-hidden="true">{letters}</span>;
}
