import type { ReactNode } from 'react';
import { Boxes } from 'lucide-react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/Card';

interface AuthShellProps {
  title: string;
  subtitle?: ReactNode;
  children: ReactNode;
  /** Rendered under the card — usually a link to another auth screen. */
  footer?: ReactNode;
}

/**
 * The centred card the signed-out screens share. Sign-in, sign-up, forgot
 * password and reset password are all one short form under a heading, and
 * having them drift apart visually would make the flow feel like it left the
 * application.
 */
export function AuthShell({ title, subtitle, children, footer }: AuthShellProps) {
  return (
    <div className="flex min-h-screen items-center justify-center bg-slate-100 p-4">
      <div className="w-full max-w-md space-y-6">
        <div className="flex flex-col items-center gap-2">
          <div className="flex h-12 w-12 items-center justify-center rounded-xl bg-slate-900 text-white">
            <Boxes className="h-6 w-6" />
          </div>
          <h1 className="text-xl font-bold tracking-tight text-slate-900">ERP System</h1>
        </div>

        <Card>
          <CardHeader>
            <CardTitle className="text-xl">{title}</CardTitle>
            {subtitle && <p className="text-sm text-slate-500">{subtitle}</p>}
          </CardHeader>
          <CardContent>
            {children}
            {footer && <div className="mt-4 text-center text-sm text-slate-600">{footer}</div>}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
