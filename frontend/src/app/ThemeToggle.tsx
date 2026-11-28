import { useLayoutEffect, useState } from 'react';

type ThemePreference = 'system' | 'light' | 'dark';
const storageKey = 'crm-theme-preference';

function readPreference(): ThemePreference {
  try {
    const saved = globalThis.localStorage.getItem(storageKey);
    return saved === 'light' || saved === 'dark' ? saved : 'system';
  } catch {
    return 'system';
  }
}

export function ThemeToggle() {
  const [preference, setPreference] = useState<ThemePreference>(readPreference);

  useLayoutEffect(() => {
    const media = globalThis.matchMedia?.('(prefers-color-scheme: dark)');
    const apply = () => {
      const effective = preference === 'system' ? (media?.matches ? 'dark' : 'light') : preference;
      globalThis.document.documentElement.dataset.theme = effective;
    };
    apply();
    if (preference === 'system') media?.addEventListener?.('change', apply);
    try {
      globalThis.localStorage.setItem(storageKey, preference);
    } catch {
      // Theme is optional. Browser storage can be disabled without blocking CRM.
    }
    return () => {
      media?.removeEventListener?.('change', apply);
    };
  }, [preference]);

  return (
    <label className="theme-control">
      <span>Theme</span>
      <select aria-label="Color theme" value={preference} onChange={(event) => setPreference(event.target.value as ThemePreference)}>
        <option value="system">System</option>
        <option value="light">Light</option>
        <option value="dark">Dark</option>
      </select>
    </label>
  );
}
