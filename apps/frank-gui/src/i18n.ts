/**
 * UI copy is addressed by stable translation keys even though the MVP ships
 * English only. Keeping copy out of the control-flow code makes a future
 * locale addition a data change instead of a risky command/bridge rewrite.
 */
const EN = {
  "nav.overview": "Overview",
  "nav.personas": "Personas",
  "nav.integrations": "Integrations",
  "nav.settings": "Settings & diagnostics",
  "app.name": "Frank",
  "app.loading": "Loading Frank…",
  "app.refresh": "Refresh",
  "app.refreshSnapshot": "Refresh snapshot",
  "settings.behavior": "Behavior",
  "settings.defaultLevel": "Default level",
  "settings.defaultLevelHelp": "Used when Frank is activated without an explicit level.",
  "settings.off": "Off",
} as const;

export type TranslationKey = keyof typeof EN;

export function t(key: TranslationKey): string {
  return EN[key];
}
