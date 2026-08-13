import { useQuery } from '@tanstack/react-query';
import { http } from '@/api/client';
import { users } from './useSettings';
import type { DocumentBody } from './useResource';
import { createResource, useAction } from './useResource';
import type {
  Employee,
  EmployeeDetail,
  ExpenseReport,
  ExpenseReportDetail,
  LeaveBalance,
  LeaveRequest,
  Uuid,
} from '@/types';

export const employees = createResource<Employee, EmployeeDetail>('/hr/employees', 'employees');
export const leaveRequests = createResource<LeaveRequest>('/hr/leave-requests', 'leave-requests');
export const expenseReports = createResource<ExpenseReport, ExpenseReportDetail, DocumentBody, DocumentBody>(
  '/hr/expense-reports',
  'expense-reports'
);

export function useLeaveBalance(employeeId: Uuid | undefined) {
  return useQuery({
    queryKey: ['employees', 'leave-balance', employeeId],
    queryFn: () => http.get<LeaveBalance>(`/hr/employees/${employeeId}/leave-balance`),
    enabled: Boolean(employeeId),
  });
}

/** Approving leave changes the employee's remaining balance too. */
export function useDecideLeave() {
  return useAction<LeaveRequest, { id: Uuid; status: 'approved' | 'rejected' }>(
    'leave-requests',
    ({ id, status }) => http.put<LeaveRequest>(`/hr/leave-requests/${id}/decision`, { status }),
    { successMessage: 'Leave request decided', invalidateKeys: ['employees'] }
  );
}

export function useExpenseStatus() {
  return useAction<ExpenseReport, { id: Uuid; status: string }>(
    'expense-reports',
    ({ id, status }) => http.put<ExpenseReport>(`/hr/expense-reports/${id}/status`, { status }),
    { successMessage: 'Expense report updated' }
  );
}

/**
 * Logins an employee can be linked to.
 *
 * The link is what scopes somebody's leave and expenses to them; without it the
 * HR module has nothing of their own to show. Admin-only, like the user list it
 * reads, so the field simply offers nothing to anyone else.
 */
export function useUserOptions() {
  const { data, isLoading } = users.useList({ per_page: 200, is_active: true, sort: 'last_name' });
  return {
    isLoading,
    options: (data?.data ?? []).map((user) => ({
      value: user.id,
      label: `${user.first_name} ${user.last_name} (${user.email})`,
    })),
  };
}

export function useEmployeeOptions() {
  const { data, isLoading } = employees.useList({
    per_page: 200,
    status: 'active',
    sort: 'last_name',
  });
  return {
    isLoading,
    employees: data?.data ?? [],
    options: (data?.data ?? []).map((employee) => ({
      value: employee.id,
      label: `${employee.first_name} ${employee.last_name} (${employee.employee_number})`,
    })),
  };
}
