import type { ReactNode } from 'react';
import { useTheme } from '../hooks/useTheme';
import { useLang } from '../i18n';

const NAV = [
  { label: 'Dashboard', href: '#/' },
  { label: 'Settings', href: '#/settings' },
];

interface Props {
  children: ReactNode;
  actions?: ReactNode;
  footer?: ReactNode;
}

function isActive(href: string): boolean {
  const hash = window.location.hash || '#/';
  if (href === '#/') return hash === '#/' || hash === '';
  return hash.startsWith(href);
}

function Logo() {
  return (
    <div className="w-9 h-9 shrink-0 rounded-[10px] flex items-center justify-center bg-[var(--app-accent-soft)] border border-[var(--app-accent-border)]">
      <svg
        className="w-4.5 h-4.5 text-accent"
        style={{ width: 18, height: 18 }}
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
      >
        <path d="M4 17V7" />
        <path d="M9 17v-6" />
        <path d="M14 17V10" />
        <path d="M19 17V4" />
      </svg>
    </div>
  );
}

export default function Layout({ children, actions, footer }: Props) {
  const { theme, toggleTheme } = useTheme();
  const { lang, setLang } = useLang();

  return (
    <div className="min-h-screen max-w-7xl mx-auto px-3 pt-5 pb-16 sm:px-6 sm:pt-7 space-y-4 sm:space-y-5 overflow-x-hidden">
      <header className="flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:gap-8 min-w-0">
          <a href="#/" className="flex items-center gap-3 min-w-0 group">
            <Logo />
            <div className="min-w-0">
              <h1 className="text-lg font-bold text-heading tracking-tight leading-tight truncate">
                AllTokens
              </h1>
              <p className="text-xs text-faint hidden sm:block leading-tight">
                AI API token usage &amp; cost
              </p>
            </div>
          </a>

          <nav className="pill w-full sm:w-auto overflow-x-auto">
            {NAV.map((item) => (
              <a
                key={item.href}
                href={item.href}
                className={`pill-item ${isActive(item.href) ? 'pill-item-active' : ''}`}
              >
                {item.label}
              </a>
            ))}
          </nav>
        </div>

        <div className="flex items-center gap-2 shrink-0 self-end sm:self-auto">
          {actions}
          <div className="pill" role="group" aria-label="Language">
            <button
              type="button"
              onClick={() => setLang('zh')}
              className={`pill-item ${lang === 'zh' ? 'pill-item-active' : ''}`}
            >
              中
            </button>
            <button
              type="button"
              onClick={() => setLang('en')}
              className={`pill-item ${lang === 'en' ? 'pill-item-active' : ''}`}
            >
              EN
            </button>
          </div>
          <button
            type="button"
            onClick={toggleTheme}
            className="icon-btn"
            title={theme === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'}
          >
            {theme === 'dark' ? (
              <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <circle cx="12" cy="12" r="5" />
                <line x1="12" y1="1" x2="12" y2="3" />
                <line x1="12" y1="21" x2="12" y2="23" />
                <line x1="4.22" y1="4.22" x2="5.64" y2="5.64" />
                <line x1="18.36" y1="18.36" x2="19.78" y2="19.78" />
                <line x1="1" y1="12" x2="3" y2="12" />
                <line x1="21" y1="12" x2="23" y2="12" />
                <line x1="4.22" y1="19.78" x2="5.64" y2="18.36" />
                <line x1="18.36" y1="5.64" x2="19.78" y2="4.22" />
              </svg>
            ) : (
              <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
              </svg>
            )}
          </button>
        </div>
      </header>

      {children}

      {footer}
    </div>
  );
}
