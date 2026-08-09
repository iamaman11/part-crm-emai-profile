import type { ClientHistoryProjection } from '../../shared/api/generated/control-plane';

export function ClientHistoryPanel({ history }: { history: ClientHistoryProjection | undefined }) {
  return (
    <section className="panel full-span">
      <span className="eyebrow">Grant-safe secondary projections</span>
      <h2>Client history</h2>
      {!history ? (
        <p>Select a visible client to load contact, assignment and activity history.</p>
      ) : (
        <div className="workspace-grid">
          <article>
            <h3>Contacts</h3>
            {history.contacts.length === 0 ? <p>No contact metadata.</p> : (
              <ul>
                {history.contacts.map((contact) => (
                  <li key={contact.contactPointId}>
                    <strong>{contact.kind}</strong> · {contact.status} · <code>{contact.contactPointId}</code>
                  </li>
                ))}
              </ul>
            )}
          </article>
          <article>
            <h3>Profile assignments</h3>
            {history.assignments.length === 0 ? <p>No visible assignment history.</p> : (
              <ul>
                {history.assignments.map((assignment) => (
                  <li key={assignment.assignmentId}>
                    <strong>{assignment.status}</strong> · <code>{assignment.profileId}</code> · {assignment.reason}
                  </li>
                ))}
              </ul>
            )}
          </article>
          <article>
            <h3>Activity and grant history</h3>
            {history.activity.length === 0 ? <p>No bounded activity.</p> : (
              <ul>
                {history.activity.map((item) => (
                  <li key={item.auditEventId}>
                    <strong>{item.action}</strong> · {item.resultCode} · {item.resourceType}/{item.resourceId}
                  </li>
                ))}
              </ul>
            )}
          </article>
        </div>
      )}
    </section>
  );
}
