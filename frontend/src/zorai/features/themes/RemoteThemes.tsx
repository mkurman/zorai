import { useEffect, useRef, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { useSettingsStore } from "@/lib/settingsStore";
import { isVideoBackground } from "@/lib/userThemes";
import { refreshUserThemes } from "./userThemesStore";

type Staged = { token: string; sourceUrl: string; record: ZoraiUserThemeRecord };
type GalleryEntry = { name: string; url: string; author: string; description: string; appearance: "dark" | "light" | null };

const errorText = (reason: unknown) => (reason instanceof Error ? reason.message : String(reason)).replace(/^Error invoking remote method '[^']+': (Error: )?/, "");

export function RemoteThemes({ onInstalled }: { onInstalled: (theme: { name: string; appearance: "dark" | "light"; hasCss: boolean }) => void }) {
  const bridge = getBridge();
  const galleryUrl = useSettingsStore((state) => state.settings.themeGalleryUrl);
  const updateSetting = useSettingsStore((state) => state.updateSetting);
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [staged, setStaged] = useState<Staged | null>(null);
  const [gallery, setGallery] = useState<GalleryEntry[] | null>(null);
  const stagedRef = useRef<Staged | null>(null);
  stagedRef.current = staged;

  useEffect(() => () => {
    if (stagedRef.current) void getBridge()?.themesDiscardStaged?.(stagedRef.current.token);
  }, []);

  if (!bridge?.themesFetchUrl) return null;

  const discard = () => {
    if (staged) void bridge.themesDiscardStaged?.(staged.token);
    setStaged(null);
  };

  const fetchTheme = async (target: string) => {
    discard();
    setBusy(true);
    setError(null);
    try {
      setStaged(await bridge.themesFetchUrl!(target));
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      setBusy(false);
    }
  };

  const install = async () => {
    if (!staged?.record.theme) return;
    setBusy(true);
    setError(null);
    try {
      const record = await bridge.themesInstallStaged!(staged.token);
      setStaged(null);
      setUrl("");
      await refreshUserThemes();
      if (record.theme) onInstalled({ name: record.theme.name, appearance: record.theme.appearance, hasCss: Boolean(record.css) });
    } catch (reason) {
      setError(errorText(reason));
      setStaged(null);
    } finally {
      setBusy(false);
    }
  };

  const browse = async () => {
    setBusy(true);
    setError(null);
    try {
      setGallery(await bridge.themesFetchGallery!(galleryUrl));
    } catch (reason) {
      setError(errorText(reason));
      setGallery(null);
    } finally {
      setBusy(false);
    }
  };

  const theme = staged?.record.theme;

  return (
    <>
      <div className="zorai-setting-row">
        <div>
          <strong>Install from the web</strong>
          <span>Paste a link to a theme.json, its folder, or a GitHub page for either. Its background and theme.css are downloaded from the same folder. Nothing is installed until you confirm.</span>
        </div>
        <form className="zorai-remote-theme__form" onSubmit={(event) => { event.preventDefault(); if (url.trim()) void fetchTheme(url.trim()); }}>
          <input className="zorai-input" value={url} placeholder="https://github.com/…/theme.json" spellCheck={false} disabled={busy} onChange={(event) => setUrl(event.target.value)} />
          <button type="submit" className="zorai-ghost-button" disabled={busy || !url.trim()}>{busy && !staged ? "Fetching…" : "Fetch"}</button>
        </form>
      </div>

      {staged && theme ? (
        <div className="zorai-remote-theme__preview" role="group" aria-label={`Preview of ${theme.name}`}>
          <strong>{theme.name}</strong>
          <span>{[theme.author ? `by ${theme.author}` : "", theme.appearance === "light" ? "Light" : "Dark", theme.base ? `based on ${theme.base}` : ""].filter(Boolean).join(" · ")}</span>
          <span className="zorai-remote-theme__source">{staged.sourceUrl}</span>
          <ul>
            {staged.record.backgroundPath ? <li>Includes a background {isVideoBackground(staged.record.backgroundPath) ? "video" : "image"}.</li> : null}
            {theme.seeThrough ? <li>Asks for a see-through window.</li> : null}
            {staged.record.css ? <li className="zorai-theme-editor__warning">Includes custom CSS ({staged.record.css.split("\n").length} lines). It can restyle anything and is not checked. Only install it if you trust where it comes from.</li> : null}
          </ul>
          <div className="zorai-panel__heading-actions">
            <button type="button" className="zorai-ghost-button" disabled={busy} onClick={discard}>Cancel</button>
            <button type="button" className="zorai-primary-button" disabled={busy} onClick={() => void install()}>{busy ? "Installing…" : "Install"}</button>
          </div>
        </div>
      ) : null}

      <div className="zorai-setting-row">
        <div>
          <strong>Theme gallery</strong>
          <span>Address of a gallery index.json that lists themes, each with a name and a link. Leave empty if you don't use one.</span>
        </div>
        <form className="zorai-remote-theme__form" onSubmit={(event) => { event.preventDefault(); if (galleryUrl.trim()) void browse(); }}>
          <input className="zorai-input" value={galleryUrl} placeholder="https://…/index.json" spellCheck={false} onChange={(event) => updateSetting("themeGalleryUrl", event.target.value)} />
          <button type="submit" className="zorai-ghost-button" disabled={busy || !galleryUrl.trim()}>Browse</button>
        </form>
      </div>

      {gallery ? (
        <div className="zorai-remote-theme__gallery">
          {gallery.length === 0 ? <span>This gallery lists no themes.</span> : gallery.map((entry) => (
            <div key={entry.url} className="zorai-remote-theme__entry">
              <div>
                <strong>{entry.name}</strong>
                <span>{[entry.author ? `by ${entry.author}` : "", entry.appearance === "light" ? "Light" : entry.appearance === "dark" ? "Dark" : ""].filter(Boolean).join(" · ")}</span>
                {entry.description ? <span>{entry.description}</span> : null}
              </div>
              <button type="button" className="zorai-ghost-button" disabled={busy} onClick={() => void fetchTheme(entry.url)}>Preview</button>
            </div>
          ))}
        </div>
      ) : null}

      {error ? <p className="zorai-theme-errors" role="alert">{error}</p> : null}
    </>
  );
}
