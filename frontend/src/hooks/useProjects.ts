import { useQuery } from '@tanstack/react-query';
import { http } from '@/api/client';
import { createResource, useAction } from './useResource';
import type {
  ListParams,
  PaginatedResponse,
  Project,
  ProjectDetail,
  Task,
  TaskWithProject,
  TimeEntry,
  Uuid,
} from '@/types';

export const projects = createResource<Project, ProjectDetail>('/projects', 'projects');
export const tasks = createResource<Task, Task>('/projects/tasks', 'tasks');
export const timeEntries = createResource<TimeEntry>('/projects/time-entries', 'time-entries');

export function useProjectTasks(projectId: Uuid | undefined, params?: ListParams) {
  return useQuery<PaginatedResponse<Task>>({
    queryKey: ['tasks', 'by-project', projectId, params ?? {}],
    queryFn: () => http.list<Task>(`/projects/${projectId}/tasks`, params),
    enabled: Boolean(projectId),
  });
}

export function useProjectTimeEntries(projectId: Uuid | undefined, params?: ListParams) {
  return useQuery<PaginatedResponse<TimeEntry>>({
    queryKey: ['time-entries', 'by-project', projectId, params ?? {}],
    queryFn: () => http.list<TimeEntry>(`/projects/${projectId}/time-entries`, params),
    enabled: Boolean(projectId),
  });
}

/** Moving a task recomputes the parent project's progress. */
export function useTaskStatus() {
  return useAction<TaskWithProject, { id: Uuid; status: string }>(
    'tasks',
    ({ id, status }) => http.put<TaskWithProject>(`/projects/tasks/${id}/status`, { status }),
    { successMessage: 'Task updated', invalidateKeys: ['projects'] }
  );
}

export function useProjectStatus() {
  return useAction<Project, { id: Uuid; status: string }>(
    'projects',
    ({ id, status }) => http.put<Project>(`/projects/${id}/status`, { status }),
    { successMessage: 'Project updated' }
  );
}

/** Logged time rolls into the task's actual hours and the project totals. */
export function useCreateTimeEntry() {
  return useAction<TimeEntry, Record<string, unknown>>(
    'time-entries',
    (body) => http.post<TimeEntry>('/projects/time-entries', body),
    { successMessage: 'Time logged', invalidateKeys: ['tasks', 'projects'] }
  );
}

export function useProjectOptions() {
  const { data, isLoading } = projects.useList({ per_page: 200, sort: 'name' });
  return {
    isLoading,
    options: (data?.data ?? []).map((project) => ({
      value: project.id,
      label: `${project.project_code} — ${project.name}`,
    })),
  };
}
