import { useState, useEffect, useCallback } from 'react';
import { Routes, Route, Navigate, NavLink, useLocation } from 'react-router-dom';
import {
  LayoutDashboard, Globe, ScrollText, Shield, KeyRound, LogOut, Menu, X, Moon, Sun, Zap,
} from 'lucide-react';
import { getToken, clearToken, onAuthChange } from './lib/api';
import { cn } from './lib/utils';
import { ErrorBoundary } from './components/ErrorBoundary';
import Login from './pages/Login';
import Dashboard from './pages/Dashboard';
import Sites from './pages/Sites';
import Logs from './pages/Logs';
import Risk from './pages/Risk';
import Security from './pages/Security';

const NAV = [
  { to: '/', label: '监控', icon: LayoutDashboard },
  { to: '/sites', label: '站点', icon: Globe },
  { to: '/logs', label: '日志', icon: ScrollText },
  { to: '/risk', label: '风控', icon: Shield },
  { to: '/security', label: '安全', icon: KeyRound },
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
    <div className="min-h-screen flex flex-col bg-gray-50 dark:bg-gray-950 text-gray-900 dark:text-gray-100 transition-colors duration-300">
      {/* Header */}
      <header className="bg-gradient-to-r from-gray-900 via-gray-900 to-gray-800 dark:from-gray-800 dark:via-gray-800 dark:to-gray-900 text-white px-4 md:px-6 py-3.5 flex items-center justify-between shadow-lg">
        <div className="flex items-center gap-3">
          <button className="md:hidden p-1.5 rounded-lg hover:bg-white/10 transition-colors" onClick={() => setSideOpen(!sideOpen)} aria-label="菜单">
            {sideOpen ? <X size={20} /> : <Menu size={20} />}
          </button>
          <div className="flex items-center gap-2">
            <Zap size={20} className="text-blue-400" />
            <span className="font-bold text-lg tracking-tight">Portcullis</span>
          </div>
          <span className="px-2 py-0.5 rounded-full bg-blue-500/20 text-blue-300 text-[11px] font-medium ring-1 ring-blue-500/30">Admin</span>
        </div>
        <div className="flex items-center gap-2">
          <button onClick={toggleDark} className="p-2 rounded-lg text-gray-400 hover:text-white hover:bg-white/10 transition-all" title="切换主题">
            {dark ? <Sun size={16} /> : <Moon size={16} />}
          </button>
          <button
            onClick={() => { clearToken(); window.dispatchEvent(new Event('captcha-admin-auth-changed')); }}
            className="text-sm text-gray-400 hover:text-white flex items-center gap-1.5 px-3 py-1.5 rounded-lg hover:bg-white/10 transition-all"
          >
            <LogOut size={14} /> 退出
          </button>
        </div>
      </header>

      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar */}
        <nav className={cn(
          'fixed inset-y-0 left-0 top-[56px] z-40 w-60 bg-white/80 dark:bg-gray-900/80 backdrop-blur-xl border-r border-border/50 dark:border-gray-800/50 p-4 flex flex-col transition-all duration-300 ease-out md:relative md:top-0 md:translate-x-0',
          sideOpen ? 'translate-x-0 shadow-2xl' : '-translate-x-full'
        )}>
          <div className="space-y-1">
            {NAV.map((item) => {
              const active = item.to === '/' ? location.pathname === '/' : location.pathname.startsWith(item.to);
              return (
                <NavLink key={item.to} to={item.to} className={cn(
                  'flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-all duration-200',
                  active
                    ? 'bg-primary text-primary-foreground shadow-md shadow-primary/20'
                    : 'text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-100 hover:bg-gray-100 dark:hover:bg-gray-800/50'
                )}>
                  <item.icon size={18} className={active ? '' : 'opacity-70'} />
                  {item.label}
                  {active && <div className="ml-auto w-1.5 h-1.5 rounded-full bg-white/80" />}
                </NavLink>
              );
            })}
          </div>
          <div className="mt-auto pt-4 border-t border-border/50 dark:border-gray-800/50">
            <div className="px-3 py-2 text-[11px] text-muted-foreground flex items-center gap-2">
              <span className="pulse-dot">Portcullis v1.2.4</span>
            </div>
          </div>
        </nav>

        {/* Overlay */}
        {sideOpen && (
          <div
            className="fixed inset-0 z-30 bg-black/40 backdrop-blur-sm md:hidden fade-in"
            onClick={() => setSideOpen(false)}
          />
        )}

        {/* Main */}
        <main className="flex-1 p-4 md:p-8 overflow-auto">
          <div key={location.pathname} className="page-enter">
            <ErrorBoundary>{children}</ErrorBoundary>
          </div>
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
        <Route path="/security" element={<Security />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </Layout>
  );
}
