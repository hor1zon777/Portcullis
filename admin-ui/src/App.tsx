import { useState, useEffect, useCallback } from 'react';
import { Routes, Route, Navigate, NavLink, useLocation } from 'react-router-dom';
import {
  LayoutDashboard, Globe, ScrollText, Shield, LogOut, Menu, X, Moon, Sun,
} from 'lucide-react';
import { getToken, clearToken, onAuthChange } from './lib/api';
import { cn } from './lib/utils';
import { ErrorBoundary } from './components/ErrorBoundary';
import Login from './pages/Login';
import Dashboard from './pages/Dashboard';
import Sites from './pages/Sites';
import Logs from './pages/Logs';
import Risk from './pages/Risk';

const NAV = [
  { to: '/', label: '监控', icon: LayoutDashboard },
  { to: '/sites', label: '站点', icon: Globe },
  { to: '/logs', label: '日志', icon: ScrollText },
  { to: '/risk', label: '风控', icon: Shield },
];

function Layout({ children }: { children: React.ReactNode }) {
  const location = useLocation();
  const [sideOpen, setSideOpen] = useState(false);
  const [dark, setDark] = useState(() => document.documentElement.classList.contains('dark'));

  const toggleDark = useCallback(() => {
    const next = !dark;
    setDark(next);
    document.documentElement.classList.toggle('dark', next);
    localStorage.setItem('theme', next ? 'dark' : 'light');
  }, [dark]);

  useEffect(() => {
    if (localStorage.getItem('theme') === 'dark') {
      setDark(true);
      document.documentElement.classList.add('dark');
    }
  }, []);

  useEffect(() => setSideOpen(false), [location.pathname]);

  return (
    <div className="min-h-screen flex flex-col bg-gray-50 dark:bg-gray-950 text-gray-900 dark:text-gray-100">
      <header className="bg-gray-900 dark:bg-gray-800 text-white px-4 md:px-6 py-3 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <button className="md:hidden p-1" onClick={() => setSideOpen(!sideOpen)} aria-label="菜单">
            {sideOpen ? <X size={20} /> : <Menu size={20} />}
          </button>
          <div className="font-semibold text-lg">PoW CAPTCHA</div>
          <span className="badge bg-blue-600 text-white text-[11px]">Admin</span>
        </div>
        <div className="flex items-center gap-3">
          <button onClick={toggleDark} className="p-1 text-gray-300 hover:text-white" title="切换主题">
            {dark ? <Sun size={16} /> : <Moon size={16} />}
          </button>
          <button
            onClick={() => { clearToken(); window.dispatchEvent(new Event('captcha-admin-auth-changed')); }}
            className="text-sm text-gray-300 hover:text-white flex items-center gap-1"
          >
            <LogOut size={14} /> 退出
          </button>
        </div>
      </header>
      <div className="flex flex-1 overflow-hidden">
        <nav className={cn(
          'fixed inset-y-0 left-0 top-[52px] z-40 w-56 bg-white dark:bg-gray-900 border-r border-border dark:border-gray-800 p-3 transition-transform md:relative md:top-0 md:translate-x-0',
          sideOpen ? 'translate-x-0' : '-translate-x-full'
        )}>
          {NAV.map((item) => {
            const active = item.to === '/' ? location.pathname === '/' : location.pathname.startsWith(item.to);
            return (
              <NavLink key={item.to} to={item.to} className={cn(
                'flex items-center gap-3 px-3 py-2 rounded-md text-sm font-medium mb-1 transition-colors',
                active ? 'bg-primary text-primary-foreground' : 'text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800'
              )}>
                <item.icon size={16} /> {item.label}
              </NavLink>
            );
          })}
        </nav>
        {sideOpen && <div className="fixed inset-0 z-30 bg-black/30 md:hidden" onClick={() => setSideOpen(false)} />}
        <main className="flex-1 p-4 md:p-6 overflow-auto">
          <ErrorBoundary>{children}</ErrorBoundary>
        </main>
      </div>
    </div>
  );
}

export default function App() {
  const [authed, setAuthed] = useState(!!getToken());
  useEffect(() => onAuthChange(() => setAuthed(!!getToken())), []);
  if (!authed) return <Login onSuccess={() => setAuthed(true)} />;
  return (
    <Layout>
      <Routes>
        <Route path="/" element={<Dashboard />} />
        <Route path="/sites" element={<Sites />} />
        <Route path="/logs" element={<Logs />} />
        <Route path="/risk" element={<Risk />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </Layout>
  );
}
