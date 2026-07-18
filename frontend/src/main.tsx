import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { initTheme } from './hooks/useTheme';
import './index.css';
import App from './App';
import ErrorBoundary from './components/ErrorBoundary';
import { LanguageProvider } from './i18n';

initTheme();

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <LanguageProvider>
      <ErrorBoundary>
        <App />
      </ErrorBoundary>
    </LanguageProvider>
  </StrictMode>,
);
