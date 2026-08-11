const MAIL_CSP = [
  "default-src 'none'",
  "base-uri 'none'",
  "connect-src 'none'",
  "font-src 'none'",
  "form-action 'none'",
  "frame-src 'none'",
  'img-src data: cid:',
  "media-src 'none'",
  "object-src 'none'",
  "script-src 'none'",
  "style-src 'unsafe-inline'",
].join('; ');

const BLOCKED_ELEMENTS = new Set([
  'base',
  'embed',
  'form',
  'iframe',
  'link',
  'meta',
  'object',
  'script',
  'source',
  'video',
  'audio',
]);

const NAVIGATION_ATTRIBUTES = new Set([
  'action',
  'formaction',
  'href',
  'ping',
  'poster',
  'srcset',
  'target',
]);

const SAFE_RASTER_DATA_IMAGE = /^data:image\/(?:png|jpeg|gif|webp);base64,[a-z0-9+/=\s]+$/i;

function sanitizeAttribute(element: Element, attribute: Attr): void {
  const name = attribute.name.toLowerCase();
  const value = attribute.value.trim();
  if (name.startsWith('on') || NAVIGATION_ATTRIBUTES.has(name)) {
    element.removeAttribute(attribute.name);
    return;
  }
  if (name === 'src') {
    if (!value.toLowerCase().startsWith('cid:') && !SAFE_RASTER_DATA_IMAGE.test(value)) {
      element.removeAttribute(attribute.name);
    }
    return;
  }
  if (name === 'style' && /url\s*\(|@import|expression\s*\(/i.test(value)) {
    element.removeAttribute(attribute.name);
  }
}

export function sanitizeMailHtml(source: string): string {
  const document = new DOMParser().parseFromString(source, 'text/html');
  for (const element of Array.from(document.body.querySelectorAll('*'))) {
    if (BLOCKED_ELEMENTS.has(element.tagName.toLowerCase())) {
      element.remove();
      continue;
    }
    for (const attribute of Array.from(element.attributes)) {
      sanitizeAttribute(element, attribute);
    }
  }
  return document.body.innerHTML;
}

export function safeMailSrcDoc(source: string): string {
  const sanitized = sanitizeMailHtml(source);
  return `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="${MAIL_CSP}"><meta name="referrer" content="no-referrer"></head><body>${sanitized}</body></html>`;
}

export { MAIL_CSP };
