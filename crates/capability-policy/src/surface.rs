use crate::ActivationUnit;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RuntimeSurface {
    HttpHealth,
    HttpBindings,
    HttpSession,
    HttpIdentity,
    HttpClients,
    HttpClientMailRead,
    HttpOutboundMail,
    HttpBrowserProfiles,
    HttpMailboxAdmin,
    HttpMailboxClientBinding,
    HttpMailboxBrowserBinding,
    HttpMailboxJobs,
    HttpProfileRuntimeDeviceJobs,
    HttpNotifications,
    QueueIntegrationEvents,
    QueueMailboxJobs,
    ScheduleIntegrationEvents,
    ScheduleMailboxJobs,
    ResolverIngress,
    ResolverReconciliation,
}

pub const ALL_RUNTIME_SURFACES: [RuntimeSurface; 20] = [
    RuntimeSurface::HttpHealth,
    RuntimeSurface::HttpBindings,
    RuntimeSurface::HttpSession,
    RuntimeSurface::HttpIdentity,
    RuntimeSurface::HttpClients,
    RuntimeSurface::HttpClientMailRead,
    RuntimeSurface::HttpOutboundMail,
    RuntimeSurface::HttpBrowserProfiles,
    RuntimeSurface::HttpMailboxAdmin,
    RuntimeSurface::HttpMailboxClientBinding,
    RuntimeSurface::HttpMailboxBrowserBinding,
    RuntimeSurface::HttpMailboxJobs,
    RuntimeSurface::HttpProfileRuntimeDeviceJobs,
    RuntimeSurface::HttpNotifications,
    RuntimeSurface::QueueIntegrationEvents,
    RuntimeSurface::QueueMailboxJobs,
    RuntimeSurface::ScheduleIntegrationEvents,
    RuntimeSurface::ScheduleMailboxJobs,
    RuntimeSurface::ResolverIngress,
    RuntimeSurface::ResolverReconciliation,
];

impl RuntimeSurface {
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::HttpHealth => "http.health",
            Self::HttpBindings => "http.bindings",
            Self::HttpSession => "http.session",
            Self::HttpIdentity => "http.identity",
            Self::HttpClients => "http.clients",
            Self::HttpClientMailRead => "http.client_mail_read",
            Self::HttpOutboundMail => "http.outbound_mail",
            Self::HttpBrowserProfiles => "http.browser_profiles",
            Self::HttpMailboxAdmin => "http.mailbox_admin",
            Self::HttpMailboxClientBinding => "http.mailbox_client_binding",
            Self::HttpMailboxBrowserBinding => "http.mailbox_browser_binding",
            Self::HttpMailboxJobs => "http.mailbox_jobs",
            Self::HttpProfileRuntimeDeviceJobs => "http.profile_runtime_device_jobs",
            Self::HttpNotifications => "http.notifications",
            Self::QueueIntegrationEvents => "queue.integration_events.consumer",
            Self::QueueMailboxJobs => "queue.mailbox_jobs.consumer",
            Self::ScheduleIntegrationEvents => "schedule.integration_events.dispatcher",
            Self::ScheduleMailboxJobs => "schedule.mailbox_jobs.dispatcher",
            Self::ResolverIngress => "service.mailbox_secret_resolver.ingress",
            Self::ResolverReconciliation => "schedule.mailbox_secret_resolver.reconciliation",
        }
    }

    #[must_use]
    pub const fn activation_unit(self) -> ActivationUnit {
        match self {
            Self::HttpHealth | Self::HttpBindings | Self::HttpSession => ActivationUnit::Foundation,
            Self::HttpIdentity => ActivationUnit::Identity,
            Self::HttpClients => ActivationUnit::Clients,
            Self::HttpClientMailRead => ActivationUnit::MailboxRead,
            Self::HttpOutboundMail => ActivationUnit::OutboundMail,
            Self::HttpBrowserProfiles => ActivationUnit::BrowserProfiles,
            Self::HttpMailboxAdmin | Self::ResolverIngress | Self::ResolverReconciliation => {
                ActivationUnit::MailboxAdmin
            }
            Self::HttpMailboxClientBinding => ActivationUnit::MailboxClientBinding,
            Self::HttpMailboxBrowserBinding => ActivationUnit::MailboxBrowserBinding,
            Self::HttpMailboxJobs | Self::QueueMailboxJobs | Self::ScheduleMailboxJobs => {
                ActivationUnit::MailboxJobs
            }
            Self::HttpProfileRuntimeDeviceJobs => ActivationUnit::ProfileRuntime,
            Self::HttpNotifications
            | Self::QueueIntegrationEvents
            | Self::ScheduleIntegrationEvents => ActivationUnit::Notifications,
        }
    }
}

pub(crate) const fn validate_catalog() {}

#[cfg(test)]
mod tests {
    use super::{ALL_RUNTIME_SURFACES, RuntimeSurface};
    use crate::ActivationUnit;
    use std::collections::BTreeSet;

    #[test]
    fn every_runtime_surface_has_unique_id_and_canonical_unit() {
        let ids: BTreeSet<&str> = ALL_RUNTIME_SURFACES
            .iter()
            .map(|surface| surface.id())
            .collect();
        assert_eq!(ids.len(), ALL_RUNTIME_SURFACES.len());
        assert_eq!(
            RuntimeSurface::ResolverIngress.activation_unit(),
            ActivationUnit::MailboxAdmin
        );
        assert_eq!(
            RuntimeSurface::ScheduleMailboxJobs.activation_unit(),
            ActivationUnit::MailboxJobs
        );
    }
}
