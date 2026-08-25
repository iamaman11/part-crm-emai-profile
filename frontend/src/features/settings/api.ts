import {
  getBindingProbe as getBindingProbeOperation,
  getHealth as getHealthOperation,
} from '../../shared/api/generated/operations';
import type { HealthResponse } from '../../shared/api/generated/operations';

export function getHealth(signal?: AbortSignal): Promise<HealthResponse> {
  return getHealthOperation(signal === undefined ? {} : { signal });
}

export function getBindingProbe(signal?: AbortSignal): Promise<HealthResponse> {
  return getBindingProbeOperation(signal === undefined ? {} : { signal });
}
