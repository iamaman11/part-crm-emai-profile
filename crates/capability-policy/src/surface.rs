use crate::ActivationUnit;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RuntimeSurface {
    HttpBindings,
    HttpSession,
    HttpIdentity,
    HttpClients,
    HttpClientMailRead,
    HttpOutboundMail,
    HttpBrowserProfiles,
    HttpProfileRuntimeLaunch,
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
    RuntimeSurface::HttpBindings,
    RuntimeSurface::HttpSession,
    RuntimeSurface::HttpIdentity,
    RuntimeSurface::HttpClients,
    RuntimeSurface::HttpClientMailRead,
    RuntimeSurface::HttpOutboundMail,
    RuntimeSurface::HttpBrowserProfiles,
    RuntimeSurface::HttpProfileRuntimeLaunch,
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
            Self::HttpBindings => "http.bindings",
            Self::HttpSession => "http.session",
            Self::HttpIdentity => "http.identity",
            Self::HttpClients => "http.clients",
            Self::HttpClientMailRead => "http.client_mail_read",
            Self::HttpOutboundMail => "http.outbound_mail",
            Self::HttpBrowserProfiles => "http.browser_profiles",
            Self::HttpProfileRuntimeLaunch => "http.profile_runtime_launch",
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
            Self::HttpBindings | Self::HttpSession => ActivationUnit::Foundation,
            Self::HttpIdentity => ActivationUnit::Identity,
            Self::HttpClients => ActivationUnit::Clients,
            Self::HttpClientMailRead => ActivationUnit::MailboxRead,
            Self::HttpOutboundMail => ActivationUnit::OutboundMail,
            Self::HttpBrowserProfiles => ActivationUnit::BrowserProfiles,
            Self::HttpProfileRuntimeLaunch | Self::HttpProfileRuntimeDeviceJobs => {
                ActivationUnit::ProfileRuntime
            }
            Self::HttpMailboxAdmin | Self::ResolverIngress | Self::ResolverReconciliation => {
                ActivationUnit::MailboxAdmin
            }
            Self::HttpMailboxClientBinding => ActivationUnit::MailboxClientBinding,
            Self::HttpMailboxBrowserBinding => ActivationUnit::MailboxBrowserBinding,
            Self::HttpMailboxJobs | Self::QueueMailboxJobs | Self::ScheduleMailboxJobs => {
                ActivationUnit::MailboxJobs
            }
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
            RuntimeSurface::HttpProfileRuntimeLaunch.activation_unit(),
            ActivationUnit::ProfileRuntime
        );
        assert_eq!(
            RuntimeSurface::ResolverIngress.activation_unit(),
            ActivationUnit::MailboxAdmin
        );
        assert_eq!(
            RuntimeSurface::QueueIntegrationEvents.activation_unit(),
            ActivationUnit::Notifications
        );
        assert_eq!(
            RuntimeSurface::QueueMailboxJobs.activation_unit(),
            ActivationUnit::MailboxJobs
        );
        assert_eq!(
            RuntimeSurface::ScheduleIntegrationEvents.activation_unit(),
            ActivationUnit::Notifications
        );
        assert_eq!(
            RuntimeSurface::ScheduleMailboxJobs.activation_unit(),
            ActivationUnit::MailboxJobs
        );
        assert!(
            ALL_RUNTIME_SURFACES
                .iter()
                .all(|surface| surface.id() != "http.health"),
            "health is deliberately observable without capability-profile admission"
        );
    }
}
