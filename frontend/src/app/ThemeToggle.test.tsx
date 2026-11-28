import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ThemeToggle } from './ThemeToggle';

describe('ThemeToggle', () => {
  const listeners = new Set<() => void>();
  let prefersDark = false;
  let appearanceCookie = '';
  beforeEach(() => {
    appearanceCookie = '';
    Object.defineProperty(globalThis.document, 'cookie', {
      configurable: true,
      get: () => appearanceCookie,
      set: (value: string) => { appearanceCookie = value.split(';')[0] ?? ''; },
    });
    globalThis.document.documentElement.removeAttribute('data-theme');
    listeners.clear();
    prefersDark = false;
    vi.stubGlobal('matchMedia', () => ({
      get matches() { return prefersDark; },
      addEventListener: (_event: string, handler: () => void) => { listeners.add(handler); },
      removeEventListener: (_event: string, handler: () => void) => { listeners.delete(handler); },
    }));
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    delete (globalThis.document as { cookie?: string }).cookie;
  });

  it('follows OS appearance until manually overridden', () => {
    render(<ThemeToggle />);
    expect(globalThis.document.documentElement.dataset.theme).toBe('light');
    prefersDark = true;
    listeners.forEach((handler) => handler());
    expect(globalThis.document.documentElement.dataset.theme).toBe('dark');
    fireEvent.change(screen.getByRole('combobox', { name: 'Color theme' }), { target: { value: 'light' } });
    expect(globalThis.document.documentElement.dataset.theme).toBe('light');
    expect(globalThis.document.cookie).toBe('po-appearance=light');
    expect(listeners.size).toBe(0);
  });

  it('restores a saved dark choice and allows returning to system', () => {
    globalThis.document.cookie = 'po-appearance=dark';
    render(<ThemeToggle />);
    expect(screen.getByRole('combobox', { name: 'Color theme' })).toHaveProperty('value', 'dark');
    expect(globalThis.document.documentElement.dataset.theme).toBe('dark');
    fireEvent.change(screen.getByRole('combobox', { name: 'Color theme' }), { target: { value: 'system' } });
    expect(globalThis.document.cookie).toBe('po-appearance=system');
    expect(globalThis.document.documentElement.dataset.theme).toBe('light');
  });
});
