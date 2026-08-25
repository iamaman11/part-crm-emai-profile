import {
  activateProfileGeneration as activateProfileGenerationOperation,
  assignProfileToClient as assignProfileToClientOperation,
  createProfileMetadata as createProfileMetadataOperation,
  deactivateProfileGeneration as deactivateProfileGenerationOperation,
  getProfileCoordinator as getProfileCoordinatorOperation,
  getProfileGeneration as getProfileGenerationOperation,
  getProfileMetadata as getProfileMetadataOperation,
  grantProfileAccess as grantProfileAccessOperation,
  issueProfileCoordinatorCommand as issueProfileCoordinatorCommandOperation,
  listProfiles as listProfilesOperation,
  quarantineProfileGeneration as quarantineProfileGenerationOperation,
  registerProfileGeneration as registerProfileGenerationOperation,
  revokeProfileAccess as revokeProfileAccessOperation,
  verifyProfileGeneration as verifyProfileGenerationOperation,
} from '../../shared/api/generated/operations';
import type {
  ActivateProfileGenerationRequest,
  AssignmentRequest,
  CoordinatorCommandDto as GeneratedCoordinatorCommandDto,
  CoordinatorCommandRequestDto,
  CoordinatorResponseDto,
  MutationReceipt,
  ProfileCreateRequest,
  ProfileGenerationResponse,
  ProfileGrantRequest,
  ProfileListPageDto,
  ProfileView,
  RegisterProfileGenerationRequest,
  VerifyProfileGenerationRequest,
} from '../../shared/api/generated/operations';
import { newIdempotencyKey } from '../../shared/api/idempotency';

export type CreateProfileInput = ProfileCreateRequest;
export type AssignProfileInput = AssignmentRequest;
export type SetProfileGrantInput = ProfileGrantRequest;
export type RegisterGenerationInput = RegisterProfileGenerationRequest;
export type VerifyGenerationInput = VerifyProfileGenerationRequest;
export type ChangeGenerationActivationInput = { readonly expectedProfileVersion: number };
export type QuarantineGenerationInput = { readonly expectedGenerationVersion: number };
export type ProfileProjection = ProfileView;
export type GenerationProjection = ProfileGenerationResponse;
export type CoordinatorResponse = CoordinatorResponseDto;
export type CoordinatorCommandDto = GeneratedCoordinatorCommandDto;
export type { CoordinatorCommandRequestDto, ProfileListPageDto };

export function listProfiles(
  tenantId: string,
  signal?: AbortSignal,
  cursor?: string | null,
  limit = 50,
): Promise<ProfileListPageDto> {
  return listProfilesOperation({
    tenantId,
    limit,
    ...(cursor === undefined || cursor === null ? {} : { cursor }),
    ...(signal === undefined ? {} : { signal }),
  });
}

export function getProfile(tenantId: string, profileId: string): Promise<ProfileProjection> {
  return getProfileMetadataOperation({ tenantId, profileId });
}

export function createProfile(tenantId: string, profileId: string): Promise<MutationReceipt> {
  return createProfileMetadataOperation({
    tenantId,
    body: { profileId },
    idempotencyKey: newIdempotencyKey(),
  });
}

export function assignProfile(
  tenantId: string,
  profileId: string,
  input: AssignProfileInput,
): Promise<MutationReceipt> {
  return assignProfileToClientOperation({
    tenantId,
    profileId,
    body: input,
    idempotencyKey: newIdempotencyKey(),
  });
}

export function setProfileGrant(
  tenantId: string,
  profileId: string,
  actorId: string,
  input: SetProfileGrantInput,
  revoke = false,
): Promise<MutationReceipt | undefined> {
  const command = {
    tenantId,
    profileId,
    actorId,
    body: input,
    idempotencyKey: newIdempotencyKey(),
  };
  return revoke ? revokeProfileAccessOperation(command) : grantProfileAccessOperation(command);
}

export function getGeneration(
  tenantId: string,
  profileId: string,
  generationId: string,
): Promise<GenerationProjection> {
  return getProfileGenerationOperation({ tenantId, profileId, generationId });
}

export function registerGeneration(
  tenantId: string,
  profileId: string,
  input: RegisterGenerationInput,
): Promise<MutationReceipt> {
  return registerProfileGenerationOperation({
    tenantId,
    profileId,
    body: input,
    idempotencyKey: newIdempotencyKey(),
  });
}

export function verifyGeneration(
  tenantId: string,
  profileId: string,
  generationId: string,
  input: VerifyGenerationInput,
): Promise<MutationReceipt> {
  return verifyProfileGenerationOperation({
    tenantId,
    profileId,
    generationId,
    body: input,
    idempotencyKey: newIdempotencyKey(),
  });
}

export function changeGenerationActivation(
  tenantId: string,
  profileId: string,
  generationId: string,
  expectedProfileVersion: number,
  activate: boolean,
): Promise<MutationReceipt> {
  const command = {
    tenantId,
    profileId,
    generationId,
    body: { expectedProfileVersion },
    idempotencyKey: newIdempotencyKey(),
  };
  return activate
    ? activateProfileGenerationOperation(command)
    : deactivateProfileGenerationOperation(command);
}

export function quarantineGeneration(
  tenantId: string,
  profileId: string,
  generationId: string,
  expectedGenerationVersion: number,
): Promise<MutationReceipt> {
  return quarantineProfileGenerationOperation({
    tenantId,
    profileId,
    generationId,
    body: { expectedGenerationVersion },
    idempotencyKey: newIdempotencyKey(),
  });
}

export function getCoordinator(tenantId: string, profileId: string): Promise<CoordinatorResponse> {
  return getProfileCoordinatorOperation({ tenantId, profileId });
}

export function commandCoordinator(
  tenantId: string,
  profileId: string,
  input: CoordinatorCommandRequestDto,
): Promise<CoordinatorResponse> {
  return issueProfileCoordinatorCommandOperation({ tenantId, profileId, body: input });
}
