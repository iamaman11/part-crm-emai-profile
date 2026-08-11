import { requestJson } from '../../shared/api/client';
import { mutate, pagedPath, segment } from '../../shared/api/endpoint';
import type { MutationReceipt } from '../../shared/api/generated/control-plane';
import type {
  CoordinatorCommandDto as GeneratedCoordinatorCommandDto,
  CoordinatorCommandRequestDto,
  CoordinatorResponseDto,
} from '../../shared/api/generated/coordinator';
import type { ProfileListPageDto } from '../../shared/api/generated/operator-query';
import type {
  GenerationProjectionDto,
  ProfileAssignmentRequest,
  ProfileCreateRequestDto,
  ProfileGenerationVersionRequest,
  ProfileGrantRequestDto,
  ProfileProjectionDto,
  QuarantineGenerationRequest,
  RegisterGenerationRequest,
  VerifyGenerationRequest,
} from '../../shared/api/generated/profile-generation';

export type CreateProfileInput = Omit<ProfileCreateRequestDto, 'requestDigest'>;
export type AssignProfileInput = Omit<ProfileAssignmentRequest, 'requestDigest'>;
export type SetProfileGrantInput = Omit<ProfileGrantRequestDto, 'requestDigest'>;
export type RegisterGenerationInput = Omit<RegisterGenerationRequest, 'requestDigest'>;
export type VerifyGenerationInput = Omit<VerifyGenerationRequest, 'requestDigest'>;
export type ChangeGenerationActivationInput = Omit<ProfileGenerationVersionRequest, 'requestDigest'>;
export type QuarantineGenerationInput = Omit<QuarantineGenerationRequest, 'requestDigest'>;
export type ProfileProjection = ProfileProjectionDto;
export type GenerationProjection = GenerationProjectionDto;
export type CoordinatorResponse = CoordinatorResponseDto;
export type CoordinatorCommandDto = GeneratedCoordinatorCommandDto;

export function listProfiles(
  tenantId: string,
  signal?: AbortSignal,
  cursor?: string | null,
  limit = 50,
): Promise<ProfileListPageDto | undefined> {
  return requestJson<ProfileListPageDto>(
    pagedPath(`/api/v1/tenants/${segment(tenantId)}/profiles`, cursor, limit),
    { tenantId, signal },
  );
}

export function getProfile(tenantId: string, profileId: string): Promise<ProfileProjection | undefined> {
  return requestJson<ProfileProjection>(
    `/api/v1/tenants/${segment(tenantId)}/profiles/${segment(profileId)}`,
    { tenantId },
  );
}

export function createProfile(tenantId: string, profileId: string): Promise<MutationReceipt | undefined> {
  const input: CreateProfileInput = { profileId };
  return mutate(`/api/v1/tenants/${segment(tenantId)}/profiles`, tenantId, 'POST', input);
}

export function assignProfile(
  tenantId: string,
  profileId: string,
  input: AssignProfileInput,
): Promise<MutationReceipt | undefined> {
  return mutate(`/api/v1/tenants/${segment(tenantId)}/profiles/${segment(profileId)}/assignment`, tenantId, 'PUT', input);
}

export function setProfileGrant(
  tenantId: string,
  profileId: string,
  actorId: string,
  input: SetProfileGrantInput,
  revoke = false,
): Promise<MutationReceipt | undefined> {
  return mutate(
    `/api/v1/tenants/${segment(tenantId)}/profiles/${segment(profileId)}/grants/${segment(actorId)}`,
    tenantId,
    revoke ? 'DELETE' : 'PUT',
    input,
  );
}

export function getGeneration(
  tenantId: string,
  profileId: string,
  generationId: string,
): Promise<GenerationProjection | undefined> {
  return requestJson<GenerationProjection>(
    `/api/v1/tenants/${segment(tenantId)}/profiles/${segment(profileId)}/generations/${segment(generationId)}`,
    { tenantId },
  );
}

export function registerGeneration(
  tenantId: string,
  profileId: string,
  input: RegisterGenerationInput,
): Promise<MutationReceipt | undefined> {
  return mutate(`/api/v1/tenants/${segment(tenantId)}/profiles/${segment(profileId)}/generations`, tenantId, 'POST', input);
}

export function verifyGeneration(
  tenantId: string,
  profileId: string,
  generationId: string,
  input: VerifyGenerationInput,
): Promise<MutationReceipt | undefined> {
  return mutate(
    `/api/v1/tenants/${segment(tenantId)}/profiles/${segment(profileId)}/generations/${segment(generationId)}/verify`,
    tenantId,
    'POST',
    input,
  );
}

export function changeGenerationActivation(
  tenantId: string,
  profileId: string,
  generationId: string,
  expectedProfileVersion: number,
  activate: boolean,
): Promise<MutationReceipt | undefined> {
  const action = activate ? 'activate' : 'deactivate';
  const input: ChangeGenerationActivationInput = { expectedProfileVersion };
  return mutate(
    `/api/v1/tenants/${segment(tenantId)}/profiles/${segment(profileId)}/generations/${segment(generationId)}/${action}`,
    tenantId,
    'POST',
    input,
  );
}

export function quarantineGeneration(
  tenantId: string,
  profileId: string,
  generationId: string,
  expectedGenerationVersion: number,
): Promise<MutationReceipt | undefined> {
  const input: QuarantineGenerationInput = { expectedGenerationVersion };
  return mutate(
    `/api/v1/tenants/${segment(tenantId)}/profiles/${segment(profileId)}/generations/${segment(generationId)}/quarantine`,
    tenantId,
    'POST',
    input,
  );
}

export function getCoordinator(tenantId: string, profileId: string): Promise<CoordinatorResponse | undefined> {
  return requestJson<CoordinatorResponse>(
    `/api/v1/tenants/${segment(tenantId)}/profiles/${segment(profileId)}/coordinator`,
    { tenantId },
  );
}

export function commandCoordinator(
  tenantId: string,
  profileId: string,
  input: CoordinatorCommandRequestDto,
): Promise<CoordinatorResponse | undefined> {
  return requestJson<CoordinatorResponse>(
    `/api/v1/tenants/${segment(tenantId)}/profiles/${segment(profileId)}/coordinator`,
    { tenantId, method: 'POST', body: input },
  );
}
