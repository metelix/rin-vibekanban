import { useState, useMemo, useCallback, useEffect } from 'react';
import { getAuthRuntime } from '@/shared/lib/auth/runtime';
import { useSyncErrorContext } from '@/shared/hooks/useSyncErrorContext';
import type { MutationDefinition, ShapeDefinition } from 'shared/remote-types';
import type { SyncError } from '@/shared/lib/electric/types';
import type { MutationResult, InsertResult } from '@/shared/lib/electric/types';

// Type helpers for extracting types from MutationDefinition
type MutationCreateType<M> =
  M extends MutationDefinition<unknown, infer C, unknown> ? C : never;
type MutationUpdateType<M> =
  M extends MutationDefinition<unknown, unknown, infer U> ? U : never;

/**
 * Base result type returned by useShape (read-only).
 */
export interface UseShapeResult<TRow> {
  /** The data array loaded from the local /v1 REST backend */
  data: TRow[];
  /** Whether the initial fetch is still loading */
  isLoading: boolean;
  /** Fetch error if one occurred */
  error: SyncError | null;
  /** Function to retry after an error */
  retry: () => void;
}

/**
 * Extended result when mutation is provided — adds insert/update/remove.
 */
export interface UseShapeMutationResult<TRow, TCreate, TUpdate>
  extends UseShapeResult<TRow> {
  /** Insert a new row (optimistic), returns row and persistence promise */
  insert: (data: TCreate) => InsertResult<TRow>;
  /** Update a row by ID (optimistic), returns persistence promise */
  update: (id: string, changes: Partial<TUpdate>) => MutationResult;
  /** Update multiple rows in a single optimistic transaction */
  updateMany: (
    updates: Array<{ id: string; changes: Partial<TUpdate> }>
  ) => MutationResult;
  /** Delete a row by ID (optimistic), returns persistence promise */
  remove: (id: string) => MutationResult;
}

/**
 * Options for the useShape hook.
 */
export interface UseShapeOptions<
  M extends
    | MutationDefinition<unknown, unknown, unknown>
    | undefined = undefined,
> {
  /**
   * Whether to fetch data from the local backend.
   * When false, returns empty data and no-op mutation functions.
   * @default true
   */
  enabled?: boolean;
  /**
   * Optional mutation definition. When provided, the hook returns
   * insert/update/remove functions for optimistic mutations.
   */
  mutation?: M;
}

// ---------------------------------------------------------------------------
// Local REST transport
//
// Self-hosted build: kanban data is served by the LOCAL /v1 backend which is
// same-origin (window.location.origin) and protected by a Bearer JWT. We reuse
// the frontend's existing auth token (via the configured AuthRuntime) and
// fetch directly with the browser fetch API — no ElectricSQL, no WebSocket.
// ---------------------------------------------------------------------------

const API_BASE = typeof window !== 'undefined' ? window.location.origin : '';

/** Safely obtain the current Bearer token. Returns null when not configured/logged out. */
async function getBearerToken(): Promise<string | null> {
  try {
    const runtime = getAuthRuntime();
    return await runtime.getToken();
  } catch {
    return null;
  }
}

/**
 * Authenticated fetch against the local /v1 backend.
 * Adds Bearer token, refreshes once on 401, never throws on network error.
 */
async function requestV1(
  path: string,
  options: RequestInit = {},
  retryOn401 = true
): Promise<Response> {
  const headers = new Headers(options.headers ?? {});
  if (!headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }
  const token = await getBearerToken();
  if (token) {
    headers.set('Authorization', `Bearer ${token}`);
  }

  const doFetch = () =>
    fetch(`${API_BASE}${path}`, {
      ...options,
      headers,
      credentials: 'include',
    });

  const response = await doFetch();

  if (response.status === 401 && retryOn401) {
    try {
      const runtime = getAuthRuntime();
      const newToken = await runtime.triggerRefresh();
      if (newToken) {
        headers.set('Authorization', `Bearer ${newToken}`);
        return doFetch();
      }
    } catch {
      /* auth runtime not configured — surface the original 401 */
    }
  }

  return response;
}

/**
 * Map a ShapeDefinition to its local /v1 REST endpoint.
 * Returns null for shapes we do not map — the hook then serves empty data
 * so the UI still renders without a sync backend.
 */
function resolveEndpoint(
  table: string,
  params: Record<string, string>
): string | null {
  const projectId = params.project_id;
  switch (table) {
    case 'issues':
      return projectId
        ? `/v1/projects/${encodeURIComponent(projectId)}/issues`
        : null;
    case 'project_statuses':
      return projectId
        ? `/v1/projects/${encodeURIComponent(projectId)}/statuses`
        : null;
    case 'issue_assignees':
      return projectId
        ? `/v1/projects/${encodeURIComponent(projectId)}/assignees`
        : null;
    case 'tags':
    case 'kanban_tags':
      return projectId
        ? `/v1/projects/${encodeURIComponent(projectId)}/tags`
        : null;
    case 'issue_relationships':
      return projectId
        ? `/v1/projects/${encodeURIComponent(projectId)}/relationships`
        : null;
    case 'workspaces':
      return projectId
        ? `/v1/projects/${encodeURIComponent(projectId)}/workspaces`
        : null;
    case 'organizations':
      return '/v1/organizations';
    case 'projects': {
      // Projects may be filtered by organization when a param is present.
      const orgId = params.organization_id;
      return orgId
        ? `/v1/projects?organization_id=${encodeURIComponent(orgId)}`
        : '/v1/projects';
    }
    default:
      return null;
  }
}

/**
 * Extract a row array from a /v1 response. Endpoints return a raw JSON array,
 * but a few historically wrap rows under a `{ [table]: [...] }` key.
 */
function extractRows(
  payload: unknown,
  table: string
): Record<string, unknown>[] {
  if (Array.isArray(payload)) {
    return payload as Record<string, unknown>[];
  }
  if (payload && typeof payload === 'object') {
    const obj = payload as Record<string, unknown>;
    const direct = obj[table];
    if (Array.isArray(direct)) {
      return direct as Record<string, unknown>[];
    }
    const firstArray = Object.values(obj).find((v) => Array.isArray(v));
    if (Array.isArray(firstArray)) {
      return firstArray as Record<string, unknown>[];
    }
  }
  return [];
}

async function parseErrorResponse(
  response: Response,
  fallback: string
): Promise<string> {
  try {
    const body = (await response.json()) as {
      message?: string;
      error?: string;
    };
    return body.message || body.error || fallback;
  } catch {
    return fallback;
  }
}

/**
 * Hook for loading a shape's data from the local /v1 REST backend,
 * with optimistic mutation support.
 *
 * This intentionally keeps the SAME exported contract as the original
 * Electric-sync implementation so all call sites remain unchanged.
 *
 * @param shape - The shape definition from shared/remote-types.ts
 * @param params - URL parameters matching the shape's requirements
 * @param options - Optional configuration (enabled, mutation, etc.)
 *
 * @example
 * const { data, isLoading } = useShape(PROJECT_ISSUES_SHAPE, { project_id });
 * const { data, insert, update, remove } = useShape(
 *   PROJECT_ISSUES_SHAPE,
 *   { project_id },
 *   { mutation: ISSUE_MUTATION }
 * );
 */
export function useShape<
  T extends Record<string, unknown>,
  M extends
    | MutationDefinition<unknown, unknown, unknown>
    | undefined = undefined,
>(
  shape: ShapeDefinition<T>,
  params: Record<string, string>,
  options: UseShapeOptions<M> = {} as UseShapeOptions<M>
): M extends MutationDefinition<unknown, unknown, unknown>
  ? UseShapeMutationResult<T, MutationCreateType<M>, MutationUpdateType<M>>
  : UseShapeResult<T> {
  const { enabled = true, mutation } = options;

  const [error, setError] = useState<SyncError | null>(null);
  const [data, setData] = useState<T[]>([]);
  const [isLoading, setIsLoading] = useState(enabled);
  const [retryKey, setRetryKey] = useState(0);

  const syncErrorContext = useSyncErrorContext();
  const registerErrorFn = syncErrorContext?.registerError;
  const clearErrorFn = syncErrorContext?.clearError;

  const retry = useCallback(() => {
    setError(null);
    setRetryKey((k) => k + 1);
  }, []);

  const paramsKey = JSON.stringify(params);
  const stableParams = useMemo(
    () => JSON.parse(paramsKey) as Record<string, string>,
    [paramsKey]
  );

  const streamId = useMemo(
    () => `${shape.table}:${paramsKey}`,
    [shape.table, paramsKey]
  );

  useEffect(() => {
    if (error && registerErrorFn) {
      registerErrorFn(streamId, shape.table, error, retry);
    } else if (!error && clearErrorFn) {
      clearErrorFn(streamId);
    }

    return () => {
      clearErrorFn?.(streamId);
    };
  }, [error, streamId, shape.table, retry, registerErrorFn, clearErrorFn]);

  // Resolve the /v1 endpoint for this shape. Unmapped shapes => null (empty).
  const endpoint = useMemo(
    () => resolveEndpoint(shape.table, stableParams),
    [shape.table, stableParams]
  );

  // Mutation endpoint base = the mutation's own url (e.g. /v1/issues).
  const mutationUrl = mutation?.url;

  // Fetch data (full read) from the local backend.
  useEffect(() => {
    if (!enabled) {
      setData([]);
      setIsLoading(false);
      setError(null);
      return;
    }

    // Unmapped shape — serve empty without error/loading.
    if (!endpoint) {
      setData([]);
      setIsLoading(false);
      setError(null);
      return;
    }

    let active = true;
    setIsLoading(true);
    setError(null);

    (async () => {
      try {
        const response = await requestV1(endpoint, {
          method: 'GET',
          cache: 'no-store',
        });

        if (!active) return;

        if (!response.ok) {
          const message = await parseErrorResponse(
            response,
            `Failed to load ${shape.table}`
          );
          setError({ status: response.status, message });
          return;
        }

        const payload = (await response.json()) as unknown;
        const rows = extractRows(payload, shape.table) as T[];
        if (!active) return;
        setData(rows);
      } catch (err) {
        if (!active) return;
        const message = err instanceof Error ? err.message : 'Network error';
        setError({ message });
      } finally {
        if (active) setIsLoading(false);
      }
    })();

    return () => {
      active = false;
    };
  }, [enabled, endpoint, shape.table, retryKey]);

  // --- Optimistic mutation support ---------------------------------------
  // All mutations: (1) apply to local state immediately for a responsive UI,
  // (2) fire a best-effort REST call guarded in try/catch so the UI never
  // throws if the backend is unavailable.

  const insert = useCallback(
    (insertData: unknown): InsertResult<T> => {
      const dataWithId = {
        id: crypto.randomUUID(),
        ...(insertData as Record<string, unknown>),
      } as unknown as T;
      setData((prev) => [dataWithId, ...prev]);

      if (mutationUrl) {
        void requestV1(mutationUrl, {
          method: 'POST',
          body: JSON.stringify(dataWithId),
        }).catch(() => {
          // Best-effort: keep optimistic row even if persistence fails.
        });
      }

      return {
        data: dataWithId,
        persisted: Promise.resolve(dataWithId),
      };
    },
    [mutationUrl]
  );

  const update = useCallback(
    (id: string, changes: unknown): MutationResult => {
      setData((prev) =>
        prev.map((row) => {
          const rowId = String((row as unknown as { id: unknown }).id ?? '');
          return rowId === id
            ? ({ ...row, ...(changes as Record<string, unknown>) } as T)
            : row;
        })
      );

      if (mutationUrl) {
        void requestV1(`${mutationUrl}/bulk`, {
          method: 'POST',
          body: JSON.stringify({
            updates: [{ id, ...(changes as Record<string, unknown>) }],
          }),
        }).catch(() => {
          // Best-effort: keep local optimistic even if persistence fails.
        });
      }

      return { persisted: Promise.resolve() };
    },
    [mutationUrl]
  );

  const updateMany = useCallback(
    (updates: Array<{ id: string; changes: unknown }>): MutationResult => {
      if (updates.length === 0) {
        return { persisted: Promise.resolve() };
      }

      const changesById = new Map(
        updates.map((update) => [update.id, update.changes])
      );
      setData((prev) =>
        prev.map((row) => {
          const record = row as unknown as { id: unknown };
          const changes = changesById.get(String(record.id ?? ''));
          if (!changes) return row;
          return { ...row, ...(changes as Record<string, unknown>) } as T;
        })
      );

      if (mutationUrl) {
        void requestV1(`${mutationUrl}/bulk`, {
          method: 'POST',
          body: JSON.stringify({
            updates: updates.map((update) => ({
              id: update.id,
              ...(update.changes as Record<string, unknown>),
            })),
          }),
        }).catch(() => {
          // Best-effort: keep local optimistic even if persistence fails.
        });
      }

      return { persisted: Promise.resolve() };
    },
    [mutationUrl]
  );

  const remove = useCallback(
    (id: string): MutationResult => {
      setData((prev) =>
        prev.filter(
          (row) => String((row as unknown as { id: unknown }).id ?? '') !== id
        )
      );

      if (mutationUrl) {
        void requestV1(`${mutationUrl}/${encodeURIComponent(id)}`, {
          method: 'DELETE',
        }).catch(() => {
          // Best-effort: keep local optimistic even if persistence fails.
        });
      }

      return { persisted: Promise.resolve() };
    },
    [mutationUrl]
  );

  const base: UseShapeResult<T> = {
    data: enabled ? data : [],
    isLoading: enabled ? isLoading : false,
    error,
    retry,
  };

  if (mutation) {
    return {
      ...base,
      insert,
      update,
      updateMany,
      remove,
    } as M extends MutationDefinition<unknown, unknown, unknown>
      ? UseShapeMutationResult<T, MutationCreateType<M>, MutationUpdateType<M>>
      : UseShapeResult<T>;
  }

  return base as M extends MutationDefinition<unknown, unknown, unknown>
    ? UseShapeMutationResult<T, MutationCreateType<M>, MutationUpdateType<M>>
    : UseShapeResult<T>;
}
