import { describe, expect, it } from 'vitest';
import { MAIL_CSP, safeMailSrcDoc, sanitizeMailHtml } from './safeMailHtml';

describe('safe mail HTML boundary', () => {
  it('removes active navigation, scripts, forms and remote resources', () => {
    const output = sanitizeMailHtml(`
      <base href="https://tracker.invalid/">
      <meta http-equiv="refresh" content="0;url=https://tracker.invalid/">
      <script>globalThis.pwned = true</script>
      <form action="https://tracker.invalid/"><input name="x"></form>
      <a href="https://tracker.invalid/" ping="https://tracker.invalid/p" onclick="alert(1)">open</a>
      <img src="https://tracker.invalid/pixel.png" onerror="alert(1)">
      <iframe src="https://tracker.invalid/"></iframe>
      <object data="https://tracker.invalid/"></object>
    `);
    expect(output).not.toMatch(/tracker\.invalid/i);
    expect(output).not.toMatch(/script|iframe|object|form/i);
    expect(output).not.toMatch(/href=|ping=|onclick=|onerror=/i);
  });

  it('keeps only cid and inert raster data images', () => {
    const output = sanitizeMailHtml(`
      <img id="cid" src="cid:logo@example.invalid">
      <img id="png" src="data:image/png;base64,iVBORw0KGgo=">
      <img id="svg" src="data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=">
    `);
    expect(output).toContain('cid:logo@example.invalid');
    expect(output).toContain('data:image/png;base64,iVBORw0KGgo=');
    expect(output).not.toContain('data:image/svg+xml');
  });

  it('strips CSS network fetches while preserving inert presentation styles', () => {
    const output = sanitizeMailHtml(`
      <div id="safe" style="font-weight:bold">safe</div>
      <div id="remote" style="background:url(https://tracker.invalid/pixel)">remote</div>
      <div id="import" style="@import url(https://tracker.invalid/style.css)">import</div>
    `);
    expect(output).toContain('font-weight:bold');
    expect(output).not.toContain('background:');
    expect(output).not.toContain('@import');
    expect(output).not.toContain('tracker.invalid');
  });

  it('emits an inert sandbox document with deny-by-default CSP and no instrumentation sinks', () => {
    const output = safeMailSrcDoc('<p>hello</p>');
    expect(MAIL_CSP).toContain("default-src 'none'");
    expect(MAIL_CSP).toContain("connect-src 'none'");
    expect(MAIL_CSP).toContain("script-src 'none'");
    expect(output).toContain('name="referrer" content="no-referrer"');
    expect(output).not.toMatch(/localStorage|sessionStorage|indexedDB|sendBeacon|analytics|telemetry/i);
  });
});
