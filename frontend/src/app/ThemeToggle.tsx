import { useLayoutEffect, useState } from 'react';

type ThemePreference = 'system' | 'light' | 'dark';
const preferenceCookie = 'po-appearance';

function readPreference(): ThemePreference {
  try {
    // Only a non-sensitive display choice lives here; no CRM identity or session data.
    const saved = globalThis.document.cookie.split(';')
      .map((entry) => entry.trim())
      .find((entry) => entry.startsWith(`${preferenceCookie}=`))?.slice(preferenceCookie.length + 1);
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
      const secure = globalThis.location.protocol === 'https:' ? '; Secure' : '';
      globalThis.document.cookie = `${preferenceCookie}=${preference}; Path=/; SameSite=Lax; Max-Age=31536000${secure}`;
    } catch {
      // Appearance persistence is optional and cannot block CRM.
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
