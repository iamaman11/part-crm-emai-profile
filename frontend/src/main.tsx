import React from 'react';
import ReactDOM from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { RouterProvider } from '@tanstack/react-router';
import { TenantProvider } from './app/TenantContext';
import { router } from './app/router';
import './app/styles.css';
import { NotificationRealtimeBridge } from './shared/realtime/NotificationRealtimeBridge';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: false, refetchOnWindowFocus: false },
    mutations: { retry: false },
  },
});

const root = document.getElementById('root');
if (!root) throw new Error('React root element is missing');

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <TenantProvider>
        <NotificationRealtimeBridge />
        <RouterProvider router={router} />
      </TenantProvider>
    </QueryClientProvider>
  </React.StrictMode>,
);
