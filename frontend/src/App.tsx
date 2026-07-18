import { useEffect, useState } from 'react';
import Dashboard from './pages/Dashboard';
import Settings from './pages/Settings';
import WidgetView from './components/WidgetView';

function useHashRoute(): string {
  const [route, setRoute] = useState(() => window.location.hash.slice(1) || '/');

  useEffect(() => {
    const onHashChange = () => setRoute(window.location.hash.slice(1) || '/');
    window.addEventListener('hashchange', onHashChange);
    return () => window.removeEventListener('hashchange', onHashChange);
  }, []);

  return route;
}

export default function App() {
  const route = useHashRoute();

  // Tauri 桌面小组件窗口（index.html?widget=1）渲染紧凑视图
  if (new URLSearchParams(window.location.search).get('widget') === '1') {
    return <WidgetView />;
  }

  if (route === '/settings') {
    return <Settings />;
  }

  return <Dashboard />;
}
