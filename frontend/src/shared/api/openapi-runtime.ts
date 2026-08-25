import { executeTransport } from './transport';
import type { TransportMethod } from './transport';

export type JsonSchema = Readonly<Record<string, unknown>>;

export interface RuntimeParameterSpec {
  readonly name: string;
  readonly in: 'path' | 'query' | 'header';
  readonly required: boolean;
  readonly style: 'simple' | 'form';
  readonly explode: boolean;
  readonly schema: JsonSchema;
  readonly argumentName?: string;
  readonly source?: 'correlation' | 'idempotency';
}

export interface RuntimeMediaTypeSpec {
  readonly mediaType: 'application/json' | 'application/problem+json';
  readonly schema: JsonSchema;
}

export interface RuntimeRequestBodySpec {
  readonly required: true;
  readonly content: ReadonlyArray<RuntimeMediaTypeSpec>;
}

export interface RuntimeResponseHeaderSpec {
  readonly name: string;
  readonly required: boolean;
  readonly style: 'simple';
  readonly explode: false;
  readonly schema: JsonSchema;
}

export interface RuntimeResponseSpec {
  readonly status: number;
  readonly headers: ReadonlyArray<RuntimeResponseHeaderSpec>;
  readonly content: ReadonlyArray<RuntimeMediaTypeSpec>;
}

export interface RuntimeOperationSpec {
  readonly operationId: string;
  readonly method: TransportMethod;
  readonly path: string;
  readonly parameters: ReadonlyArray<RuntimeParameterSpec>;
  readonly requestBody: RuntimeRequestBodySpec | null;
  readonly responses: ReadonlyArray<RuntimeResponseSpec>;
}

export interface RuntimeOperationResult {
  readonly status: number;
  readonly payload: unknown;
}

export class ApiProtocolError extends TypeError {
  readonly operationId: string;

  constructor(operationId: string, message: string) {
    super(`${operationId}: ${message}`);
    this.name = 'ApiProtocolError';
    this.operationId = operationId;
  }
}

export class ApiProblem extends Error {
  readonly status: number;
  readonly code: string;
  readonly correlationId: string;
  readonly problemType: string;

  constructor(payload: {
    readonly type: string;
    readonly title: string;
    readonly status: number;
    readonly code: string;
    readonly correlation_id: string;
  }) {
    super(payload.title);
    this.name = 'ApiProblem';
    this.status = payload.status;
    this.code = payload.code;
    this.correlationId = payload.correlation_id;
    this.problemType = payload.type;
  }
}

const COMPONENT_REF_PREFIX = '#/components/schemas/';
const MAX_SCHEMA_DEPTH = 128;
const textEncoder = new TextEncoder();
const textDecoder = new TextDecoder('utf-8', { fatal: true });

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function deepEqual(left: unknown, right: unknown): boolean {
  if (Object.is(left, right)) return true;
  if (Array.isArray(left) && Array.isArray(right)) {
    return left.length === right.length && left.every((value, index) => deepEqual(value, right[index]));
  }
  if (isRecord(left) && isRecord(right)) {
    const leftKeys = Object.keys(left).sort();
    const rightKeys = Object.keys(right).sort();
    return (
      leftKeys.length === rightKeys.length &&
      leftKeys.every((key, index) => key === rightKeys[index] && deepEqual(left[key], right[key]))
    );
  }
  return false;
}

function resolveSchema(
  schema: JsonSchema,
  components: Readonly<Record<string, JsonSchema>>,
): JsonSchema | null {
  const reference = schema.$ref;
  if (reference === undefined) return schema;
  if (typeof reference !== 'string' || !reference.startsWith(COMPONENT_REF_PREFIX)) return null;
  const name = reference.slice(COMPONENT_REF_PREFIX.length);
  const target = components[name];
  if (target === undefined) return null;
  const siblings = Object.entries(schema).filter(([key]) => key !== '$ref');
  return siblings.length === 0 ? target : { ...target, ...Object.fromEntries(siblings) };
}

function matchesType(type: string, value: unknown): boolean {
  switch (type) {
    case 'null':
      return value === null;
    case 'boolean':
      return typeof value === 'boolean';
    case 'string':
      return typeof value === 'string';
    case 'number':
      return typeof value === 'number' && Number.isFinite(value);
    case 'integer':
      return typeof value === 'number' && Number.isSafeInteger(value);
    case 'array':
      return Array.isArray(value);
    case 'object':
      return isRecord(value);
    default:
      return false;
  }
}

function matchesNumber(schema: JsonSchema, value: number): boolean {
  if (typeof schema.minimum === 'number' && value < schema.minimum) return false;
  if (typeof schema.maximum === 'number' && value > schema.maximum) return false;
  if (typeof schema.exclusiveMinimum === 'number' && value <= schema.exclusiveMinimum) return false;
  if (typeof schema.exclusiveMaximum === 'number' && value >= schema.exclusiveMaximum) return false;
  if (typeof schema.multipleOf === 'number') {
    if (!(schema.multipleOf > 0)) return false;
    const quotient = value / schema.multipleOf;
    if (Math.abs(quotient - Math.round(quotient)) > Number.EPSILON * Math.max(1, Math.abs(quotient))) {
      return false;
    }
  }
  return true;
}

function matchesString(schema: JsonSchema, value: string): boolean {
  if (typeof schema.minLength === 'number' && value.length < schema.minLength) return false;
  if (typeof schema.maxLength === 'number' && value.length > schema.maxLength) return false;
  if (typeof schema.pattern === 'string') {
    try {
      if (!new RegExp(schema.pattern, 'u').test(value)) return false;
    } catch {
      return false;
    }
  }
  return true;
}

function matchesArray(
  schema: JsonSchema,
  value: ReadonlyArray<unknown>,
  components: Readonly<Record<string, JsonSchema>>,
  depth: number,
): boolean {
  if (typeof schema.minItems === 'number' && value.length < schema.minItems) return false;
  if (typeof schema.maxItems === 'number' && value.length > schema.maxItems) return false;
  if (schema.uniqueItems === true) {
    for (let left = 0; left < value.length; left += 1) {
      for (let right = left + 1; right < value.length; right += 1) {
        if (deepEqual(value[left], value[right])) return false;
      }
    }
  }
  if (isRecord(schema.items)) {
    return value.every((item) => matchesSchemaInternal(schema.items as JsonSchema, item, components, depth + 1));
  }
  return schema.items === undefined;
}

function matchesObject(
  schema: JsonSchema,
  value: Record<string, unknown>,
  components: Readonly<Record<string, JsonSchema>>,
  depth: number,
): boolean {
  const keys = Object.keys(value);
  if (typeof schema.minProperties === 'number' && keys.length < schema.minProperties) return false;
  if (typeof schema.maxProperties === 'number' && keys.length > schema.maxProperties) return false;

  const required = schema.required;
  if (required !== undefined) {
    if (!Array.isArray(required) || !required.every((name) => typeof name === 'string')) return false;
    if (!required.every((name) => Object.prototype.hasOwnProperty.call(value, name))) return false;
  }

  const properties = schema.properties;
  if (properties !== undefined && !isRecord(properties)) return false;
  const known = properties ?? {};
  for (const [name, propertySchema] of Object.entries(known)) {
    if (Object.prototype.hasOwnProperty.call(value, name)) {
      if (!isRecord(propertySchema)) return false;
      if (!matchesSchemaInternal(propertySchema, value[name], components, depth + 1)) return false;
    }
  }

  const additional = schema.additionalProperties;
  const extraKeys = keys.filter((key) => !Object.prototype.hasOwnProperty.call(known, key));
  if (additional === false && extraKeys.length > 0) return false;
  if (isRecord(additional)) {
    if (!extraKeys.every((key) => matchesSchemaInternal(additional, value[key], components, depth + 1))) {
      return false;
    }
  }
  return additional === undefined || additional === true || additional === false || isRecord(additional);
}

function matchesSchemaInternal(
  original: JsonSchema,
  value: unknown,
  components: Readonly<Record<string, JsonSchema>>,
  depth: number,
): boolean {
  if (depth > MAX_SCHEMA_DEPTH) return false;
  const schema = resolveSchema(original, components);
  if (schema === null) return false;
  if (schema !== original) return matchesSchemaInternal(schema, value, components, depth + 1);

  if (Object.prototype.hasOwnProperty.call(schema, 'const') && !deepEqual(value, schema.const)) return false;
  if (Array.isArray(schema.enum) && !schema.enum.some((candidate) => deepEqual(value, candidate))) return false;

  if (Array.isArray(schema.allOf)) {
    if (!schema.allOf.every((child) => isRecord(child) && matchesSchemaInternal(child, value, components, depth + 1))) {
      return false;
    }
  }
  if (Array.isArray(schema.anyOf)) {
    if (!schema.anyOf.some((child) => isRecord(child) && matchesSchemaInternal(child, value, components, depth + 1))) {
      return false;
    }
  }
  if (Array.isArray(schema.oneOf)) {
    const matches = schema.oneOf.filter(
      (child) => isRecord(child) && matchesSchemaInternal(child, value, components, depth + 1),
    ).length;
    if (matches !== 1) return false;
  }
  if (isRecord(schema.not) && matchesSchemaInternal(schema.not, value, components, depth + 1)) return false;

  const rawType = schema.type;
  if (rawType !== undefined) {
    const types = Array.isArray(rawType) ? rawType : [rawType];
    if (!types.every((type) => typeof type === 'string')) return false;
    if (!types.some((type) => matchesType(type, value))) return false;
  }

  if (typeof value === 'number' && !matchesNumber(schema, value)) return false;
  if (typeof value === 'string' && !matchesString(schema, value)) return false;
  if (Array.isArray(value) && !matchesArray(schema, value, components, depth)) return false;
  if (isRecord(value) && !matchesObject(schema, value, components, depth)) return false;
  return true;
}

export function matchesSchema(
  schema: JsonSchema,
  value: unknown,
  components: Readonly<Record<string, JsonSchema>>,
): boolean {
  return matchesSchemaInternal(schema, value, components, 0);
}

function opaqueRequestId(prefix: 'corr' | 'idem'): string {
  return `${prefix}_${crypto.randomUUID().replaceAll('-', '')}`;
}

function inputValue(input: object, name: string): unknown {
  return Reflect.get(input, name);
}

function scalarText(value: unknown): string | null {
  if (typeof value === 'string') return value;
  if (typeof value === 'number' && Number.isFinite(value)) return String(value);
  if (typeof value === 'boolean') return value ? 'true' : 'false';
  return null;
}

function simpleValue(value: unknown, explode: boolean): string | null {
  const scalar = scalarText(value);
  if (scalar !== null) return scalar;
  if (Array.isArray(value)) {
    const items = value.map(scalarText);
    return items.every((item): item is string => item !== null) ? items.join(',') : null;
  }
  if (isRecord(value)) {
    const entries = Object.entries(value).sort(([left], [right]) => left.localeCompare(right));
    const rendered: string[] = [];
    for (const [key, child] of entries) {
      const text = scalarText(child);
      if (text === null) return null;
      if (explode) rendered.push(`${key}=${text}`);
      else rendered.push(key, text);
    }
    return rendered.join(',');
  }
  return null;
}

function appendQuery(search: URLSearchParams, parameter: RuntimeParameterSpec, value: unknown): boolean {
  const scalar = scalarText(value);
  if (scalar !== null) {
    search.append(parameter.name, scalar);
    return true;
  }
  if (Array.isArray(value)) {
    const items = value.map(scalarText);
    if (!items.every((item): item is string => item !== null)) return false;
    if (parameter.explode) items.forEach((item) => search.append(parameter.name, item));
    else search.append(parameter.name, items.join(','));
    return true;
  }
  if (isRecord(value)) {
    const entries = Object.entries(value).sort(([left], [right]) => left.localeCompare(right));
    if (parameter.explode) {
      for (const [key, child] of entries) {
        const text = scalarText(child);
        if (text === null) return false;
        search.append(key, text);
      }
      return true;
    }
    const rendered: string[] = [];
    for (const [key, child] of entries) {
      const text = scalarText(child);
      if (text === null) return false;
      rendered.push(key, text);
    }
    search.append(parameter.name, rendered.join(','));
    return true;
  }
  return false;
}

function parameterValue(parameter: RuntimeParameterSpec, input: object): unknown {
  if (parameter.source === 'correlation') return opaqueRequestId('corr');
  if (parameter.source === 'idempotency') {
    const provided = inputValue(input, 'idempotencyKey');
    return provided === undefined ? opaqueRequestId('idem') : provided;
  }
  if (parameter.argumentName === undefined) return undefined;
  return inputValue(input, parameter.argumentName);
}

function buildRequest(
  spec: RuntimeOperationSpec,
  input: object,
  components: Readonly<Record<string, JsonSchema>>,
): { path: string; headers: Headers; body?: Uint8Array; signal?: AbortSignal } {
  let path = spec.path;
  const search = new URLSearchParams();
  const headers = new Headers();

  for (const parameter of spec.parameters) {
    const value = parameterValue(parameter, input);
    if (value === undefined) {
      if (parameter.required) throw new ApiProtocolError(spec.operationId, `required ${parameter.in} parameter ${parameter.name} is missing`);
      continue;
    }
    if (!matchesSchema(parameter.schema, value, components)) {
      throw new ApiProtocolError(spec.operationId, `${parameter.in} parameter ${parameter.name} failed its OpenAPI schema`);
    }

    if (parameter.in === 'path') {
      const rendered = simpleValue(value, parameter.explode);
      if (rendered === null) throw new ApiProtocolError(spec.operationId, `path parameter ${parameter.name} is not serializable`);
      const token = `{${parameter.name}}`;
      if (!path.includes(token)) throw new ApiProtocolError(spec.operationId, `path template does not contain ${token}`);
      path = path.replaceAll(token, encodeURIComponent(rendered));
    } else if (parameter.in === 'query') {
      if (!appendQuery(search, parameter, value)) {
        throw new ApiProtocolError(spec.operationId, `query parameter ${parameter.name} is not serializable`);
      }
    } else {
      const rendered = simpleValue(value, parameter.explode);
      if (rendered === null) throw new ApiProtocolError(spec.operationId, `header ${parameter.name} is not serializable`);
      headers.set(parameter.name, rendered);
    }
  }

  if (/\{[^{}]+\}/u.test(path)) {
    throw new ApiProtocolError(spec.operationId, 'not all path placeholders were bound');
  }
  const query = search.toString();
  if (query !== '') path = `${path}?${query}`;

  const accepted = new Set<string>();
  for (const response of spec.responses) {
    response.content.forEach((content) => accepted.add(content.mediaType));
  }
  if (accepted.size > 0) headers.set('Accept', [...accepted].sort().join(', '));

  const result: { path: string; headers: Headers; body?: Uint8Array; signal?: AbortSignal } = { path, headers };
  if (spec.requestBody !== null) {
    const body = inputValue(input, 'body');
    if (body === undefined) throw new ApiProtocolError(spec.operationId, 'required request body is missing');
    const media = spec.requestBody.content[0];
    if (media === undefined) throw new ApiProtocolError(spec.operationId, 'request body has no declared media type');
    if (!matchesSchema(media.schema, body, components)) {
      throw new ApiProtocolError(spec.operationId, 'request body failed its OpenAPI schema');
    }
    let encoded: string;
    try {
      encoded = JSON.stringify(body);
    } catch {
      throw new ApiProtocolError(spec.operationId, 'request body is not JSON serializable');
    }
    headers.set('Content-Type', media.mediaType);
    result.body = textEncoder.encode(encoded);
  }

  const signal = inputValue(input, 'signal');
  if (signal !== undefined) {
    if (!(signal instanceof AbortSignal)) throw new ApiProtocolError(spec.operationId, 'signal is not an AbortSignal');
    result.signal = signal;
  }
  return result;
}

function normalizedContentType(headers: Headers): string {
  return (headers.get('content-type') ?? '').split(';', 1)[0]?.trim().toLowerCase() ?? '';
}

function decodeHeaderValue(schema: JsonSchema, raw: string): unknown {
  const type = schema.type;
  if (type === 'integer' || type === 'number') {
    const value = Number(raw);
    return Number.isFinite(value) ? value : raw;
  }
  if (type === 'boolean') {
    if (raw === 'true') return true;
    if (raw === 'false') return false;
  }
  return raw;
}

function validateResponseHeaders(
  spec: RuntimeOperationSpec,
  response: RuntimeResponseSpec,
  headers: Headers,
  components: Readonly<Record<string, JsonSchema>>,
): void {
  for (const header of response.headers) {
    const raw = headers.get(header.name);
    if (raw === null) {
      if (header.required) throw new ApiProtocolError(spec.operationId, `response ${response.status} omitted required header ${header.name}`);
      continue;
    }
    if (!matchesSchema(header.schema, decodeHeaderValue(header.schema, raw), components)) {
      throw new ApiProtocolError(spec.operationId, `response header ${header.name} failed its OpenAPI schema`);
    }
  }
}

function problemFromUnknown(spec: RuntimeOperationSpec, status: number, payload: unknown): ApiProblem {
  if (!isRecord(payload)) throw new ApiProtocolError(spec.operationId, `declared error ${status} did not decode to a problem object`);
  const type = payload.type;
  const title = payload.title;
  const problemStatus = payload.status;
  const code = payload.code;
  const correlationId = payload.correlation_id;
  if (
    typeof type !== 'string' ||
    typeof title !== 'string' ||
    typeof problemStatus !== 'number' ||
    !Number.isInteger(problemStatus) ||
    typeof code !== 'string' ||
    typeof correlationId !== 'string'
  ) {
    throw new ApiProtocolError(spec.operationId, `declared error ${status} is not a recognized problem payload`);
  }
  if (problemStatus !== status) {
    throw new ApiProtocolError(spec.operationId, `problem status ${problemStatus} does not match HTTP status ${status}`);
  }
  return new ApiProblem({ type, title, status: problemStatus, code, correlation_id: correlationId });
}

export async function invokeOperation(
  spec: RuntimeOperationSpec,
  input: object,
  components: Readonly<Record<string, JsonSchema>>,
): Promise<RuntimeOperationResult> {
  const request = buildRequest(spec, input, components);
  const transport = await executeTransport({
    method: spec.method,
    path: request.path,
    headers: request.headers,
    ...(request.body === undefined ? {} : { body: request.body }),
    ...(request.signal === undefined ? {} : { signal: request.signal }),
  });

  const response = spec.responses.find((candidate) => candidate.status === transport.status);
  if (response === undefined) {
    throw new ApiProtocolError(spec.operationId, `received undeclared HTTP status ${transport.status}`);
  }
  validateResponseHeaders(spec, response, transport.headers, components);

  let payload: unknown = undefined;
  if (response.content.length === 0) {
    if (transport.bytes.byteLength !== 0) {
      throw new ApiProtocolError(spec.operationId, `response ${response.status} declared no body but returned bytes`);
    }
  } else {
    const contentType = normalizedContentType(transport.headers);
    const media = response.content.find((candidate) => candidate.mediaType === contentType);
    if (media === undefined) {
      throw new ApiProtocolError(spec.operationId, `response ${response.status} used undeclared media type ${contentType || '<missing>'}`);
    }
    let text: string;
    try {
      text = textDecoder.decode(transport.bytes);
    } catch {
      throw new ApiProtocolError(spec.operationId, 'response body is not valid UTF-8');
    }
    try {
      payload = JSON.parse(text) as unknown;
    } catch {
      throw new ApiProtocolError(spec.operationId, 'response body is not valid JSON');
    }
    if (!matchesSchema(media.schema, payload, components)) {
      throw new ApiProtocolError(spec.operationId, `response ${response.status} failed its OpenAPI schema`);
    }
  }

  if (transport.status >= 400) {
    throw problemFromUnknown(spec, transport.status, payload);
  }
  return { status: transport.status, payload };
}
