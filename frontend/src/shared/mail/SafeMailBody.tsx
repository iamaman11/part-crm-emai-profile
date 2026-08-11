import { safeMailSrcDoc } from './safeMailHtml';

type Props = {
  textBody: string | null;
  htmlBody: string | null;
  title?: string;
};

export function SafeMailBody({ textBody, htmlBody, title = 'Message body' }: Props) {
  if (htmlBody) {
    return (
      <iframe
        title={title}
        sandbox=""
        referrerPolicy="no-referrer"
        srcDoc={safeMailSrcDoc(htmlBody)}
        className="mail-body-frame"
      />
    );
  }

  if (textBody) {
    return <pre className="mail-body-text">{textBody}</pre>;
  }

  return <p className="muted">This message has no displayable body.</p>;
}
