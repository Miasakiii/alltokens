import { Component, type ReactNode } from 'react';
import { useLang } from '../i18n';

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
}

interface InnerProps extends Props {
  zh: boolean;
}

/**
 * Class error boundary. Hooks are unavailable in class components, so the
 * default-export function wrapper injects the current language as a prop.
 */
class ErrorBoundaryInner extends Component<InnerProps, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: { componentStack: string }) {
    console.error('[alltokens] render error:', error, info.componentStack);
  }

  render() {
    const { zh } = this.props;
    if (this.state.error) {
      return (
        <div className="min-h-screen flex items-center justify-center p-6">
          <div className="surface p-8 max-w-md w-full text-center">
            <div
              className="mx-auto w-12 h-12 rounded-[12px] flex items-center justify-center mb-4"
              style={{
                background: 'var(--app-danger-soft)',
                border: '1px solid var(--app-surface-border)',
              }}
            >
              <svg
                className="w-6 h-6 text-danger"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.8"
                strokeLinecap="round"
                strokeLinejoin="round"
                aria-hidden="true"
              >
                <path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
                <line x1="12" y1="9" x2="12" y2="13" />
                <line x1="12" y1="17" x2="12.01" y2="17" />
              </svg>
            </div>
            <h1 className="text-lg font-semibold text-heading">
              {zh ? '出错了' : 'Something went wrong'}
            </h1>
            <p className="mt-2 text-sm text-muted break-all">
              {this.state.error.message || (zh ? '发生意外的渲染错误' : 'Unexpected render error')}
            </p>
            <button
              type="button"
              onClick={() => window.location.reload()}
              className="btn btn-primary mt-5"
            >
              {zh ? '重载页面' : 'Reload dashboard'}
            </button>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}

/**
 * Global error boundary — renders a friendly recovery card instead of a blank
 * page when any descendant throws during render.
 */
export default function ErrorBoundary({ children }: Props) {
  const zh = useLang().lang === 'zh';
  return <ErrorBoundaryInner zh={zh}>{children}</ErrorBoundaryInner>;
}
