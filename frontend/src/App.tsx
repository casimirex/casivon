import { Navigate, Route, Routes } from 'react-router-dom';
import { useAuthStore } from '@/store/authStore';
import { AppLayout } from '@/components/layout/AppLayout';
import { Login } from '@/pages/Login';
import { ForgotPassword } from '@/pages/ForgotPassword';
import { ResetPassword } from '@/pages/ResetPassword';
import { VerifyEmail } from '@/pages/VerifyEmail';
import { Dashboard } from '@/pages/Dashboard';
import { CRMPage } from '@/pages/crm/CRMPage';
import { QuoteList } from '@/pages/sales/QuoteList';
import { QuoteForm } from '@/pages/sales/QuoteForm';
import { QuoteDetail } from '@/pages/sales/QuoteDetail';
import { OrderDetail, OrderList } from '@/pages/sales/OrderPages';
import {
  CreditNoteList,
  InvoiceDetail,
  InvoiceList,
  PaymentList,
} from '@/pages/sales/InvoicePages';
import { ProductDetail, ProductList } from '@/pages/inventory/ProductPages';
import { MovementList, WarehouseList } from '@/pages/inventory/StockPages';
import { BomDetail, BomForm, BomList } from '@/pages/inventory/BomPages';
import { VendorList } from '@/pages/purchasing/VendorList';
import {
  GoodsReceiptList,
  PurchaseOrderDetail,
  PurchaseOrderForm,
  PurchaseOrderList,
  PurchaseReturnList,
} from '@/pages/purchasing/PurchaseOrderPages';
import {
  BankAccountList,
  ChartOfAccounts,
  GeneralLedger,
  TaxRateList,
} from '@/pages/accounting/AccountPages';
import { Reports } from '@/pages/accounting/Reports';
import {
  EmployeeDetail,
  EmployeeList,
  ExpenseReportList,
  LeaveRequestList,
} from '@/pages/hr/HrPages';
import { ProjectDetail, ProjectList } from '@/pages/projects/ProjectPages';
import { SettingsLayout } from '@/pages/settings/SettingsLayout';
import { ProfileSettings } from '@/pages/settings/ProfileSettings';
import { UserSettings } from '@/pages/settings/UserSettings';
import { OrganizationSettings } from '@/pages/settings/OrganizationSettings';
import { ExchangeRateSettings } from '@/pages/settings/ExchangeRateSettings';
import { PostingSettings } from '@/pages/settings/PostingSettings';
import { EmptyState } from '@/components/common/States';

function ProtectedRoute({ children }: { children: React.ReactNode }) {
  const isAuthenticated = useAuthStore((state) => state.isAuthenticated);
  return isAuthenticated ? <>{children}</> : <Navigate to="/login" replace />;
}

/** Blocks a whole area for users without the role, rather than 403-ing per call. */
function RoleRoute({ roles, children }: { roles: string[]; children: React.ReactNode }) {
  const allowed = useAuthStore((state) => state.hasRole(...roles));

  if (!allowed) {
    return (
      <EmptyState
        title="You do not have access to this area"
        message={`This section is limited to: ${roles.join(', ')}. Ask an administrator to grant you the role.`}
      />
    );
  }

  return <>{children}</>;
}

function App() {
  return (
    <Routes>
      <Route path="/login" element={<Login />} />
      <Route path="/forgot-password" element={<ForgotPassword />} />
      <Route path="/reset-password" element={<ResetPassword />} />
      {/* Opened from a mail client, so it must work without a session. */}
      <Route path="/verify-email" element={<VerifyEmail />} />

      <Route
        path="/"
        element={
          <ProtectedRoute>
            <AppLayout />
          </ProtectedRoute>
        }
      >
        <Route index element={<Dashboard />} />
        <Route path="crm" element={<CRMPage />} />

        {/* Sales */}
        <Route path="sales" element={<Navigate to="/sales/quotes" replace />} />
        <Route path="sales/quotes" element={<QuoteList />} />
        <Route path="sales/quotes/new" element={<QuoteForm />} />
        <Route path="sales/quotes/:id" element={<QuoteDetail />} />
        <Route path="sales/quotes/:id/edit" element={<QuoteForm />} />
        <Route path="sales/orders" element={<OrderList />} />
        <Route path="sales/orders/:id" element={<OrderDetail />} />
        <Route path="sales/invoices" element={<InvoiceList />} />
        <Route path="sales/invoices/:id" element={<InvoiceDetail />} />
        <Route path="sales/payments" element={<PaymentList />} />
        <Route path="sales/credit-notes" element={<CreditNoteList />} />

        {/* Inventory */}
        <Route path="inventory" element={<Navigate to="/inventory/products" replace />} />
        <Route path="inventory/products" element={<ProductList />} />
        <Route path="inventory/products/:id" element={<ProductDetail />} />
        <Route path="inventory/warehouses" element={<WarehouseList />} />
        <Route path="inventory/movements" element={<MovementList />} />
        <Route path="inventory/boms" element={<BomList />} />
        <Route path="inventory/boms/new" element={<BomForm />} />
        <Route path="inventory/boms/:id" element={<BomDetail />} />

        {/* Purchasing */}
        <Route path="purchasing" element={<Navigate to="/purchasing/vendors" replace />} />
        <Route path="purchasing/vendors" element={<VendorList />} />
        <Route path="purchasing/purchase-orders" element={<PurchaseOrderList />} />
        <Route path="purchasing/purchase-orders/new" element={<PurchaseOrderForm />} />
        <Route path="purchasing/purchase-orders/:id" element={<PurchaseOrderDetail />} />
        <Route path="purchasing/goods-receipts" element={<GoodsReceiptList />} />
        <Route path="purchasing/purchase-returns" element={<PurchaseReturnList />} />

        {/* Accounting — the API gates these on the accountant/manager role too. */}
        <Route
          path="accounting"
          element={<Navigate to="/accounting/accounts" replace />}
        />
        <Route
          path="accounting/accounts"
          element={
            <RoleRoute roles={['accountant', 'manager']}>
              <ChartOfAccounts />
            </RoleRoute>
          }
        />
        <Route
          path="accounting/ledger"
          element={
            <RoleRoute roles={['accountant', 'manager']}>
              <GeneralLedger />
            </RoleRoute>
          }
        />
        <Route
          path="accounting/bank-accounts"
          element={
            <RoleRoute roles={['accountant', 'manager']}>
              <BankAccountList />
            </RoleRoute>
          }
        />
        <Route
          path="accounting/tax-rates"
          element={
            <RoleRoute roles={['accountant', 'manager']}>
              <TaxRateList />
            </RoleRoute>
          }
        />
        <Route
          path="accounting/reports"
          element={
            <RoleRoute roles={['accountant', 'manager']}>
              <Reports />
            </RoleRoute>
          }
        />

        {/* HR */}
        <Route path="hr" element={<Navigate to="/hr/employees" replace />} />
        {/* The directory returns salaries, so the API restricts it. The UI
            offered the screen anyway and showed a 403. */}
        <Route
          path="hr/employees"
          element={
            <RoleRoute roles={['hr', 'manager']}>
              <EmployeeList />
            </RoleRoute>
          }
        />
        <Route path="hr/employees/:id" element={<EmployeeDetail />} />
        <Route path="hr/leave-requests" element={<LeaveRequestList />} />
        <Route path="hr/expense-reports" element={<ExpenseReportList />} />

        {/* Projects */}
        <Route path="projects" element={<ProjectList />} />
        <Route path="projects/:id" element={<ProjectDetail />} />

        {/* Settings */}
        <Route path="settings" element={<SettingsLayout />}>
          <Route index element={<Navigate to="/settings/profile" replace />} />
          <Route path="profile" element={<ProfileSettings />} />
          <Route
            path="users"
            element={
              <RoleRoute roles={['admin']}>
                <UserSettings />
              </RoleRoute>
            }
          />
          <Route
            path="company"
            element={
              <RoleRoute roles={['admin']}>
                <OrganizationSettings />
              </RoleRoute>
            }
          />
          <Route
            path="exchange-rates"
            element={
              <RoleRoute roles={['admin']}>
                <ExchangeRateSettings />
              </RoleRoute>
            }
          />
          <Route
            path="posting"
            element={
              <RoleRoute roles={['admin']}>
                <PostingSettings />
              </RoleRoute>
            }
          />
        </Route>

        <Route
          path="*"
          element={
            <EmptyState
              title="Page not found"
              message="The link you followed does not point anywhere in this app."
            />
          }
        />
      </Route>
    </Routes>
  );
}

export default App;
