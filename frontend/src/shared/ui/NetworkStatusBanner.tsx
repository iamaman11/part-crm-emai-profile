import { useEffect, useState } from 'react';

export function NetworkStatusBanner() {
  const [online, setOnline] = useState(() => navigator.onLine);

  useEffect(() => {
    const handleOnline = () => setOnline(true);
    const handleOffline = () => setOnline(false);
    window.addEventListener('online', handleOnline);
    window.addEventListener('offline', handleOffline);
    return () => {
      window.removeEventListener('online', handleOnline);
      window.removeEventListener('offline', handleOffline);
    };
  }, []);

  if (online) return null;

  return (
    <div className="status-banner" role="status">
      Offline: canonical server data and governed mutations are unavailable. Existing screen state is
      not fresh authority and will be revalidated after connectivity returns.
    </div>
  );
}
