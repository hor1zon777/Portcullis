import { Component, type ErrorInfo, type ReactNode } from 'react';
import { AlertTriangle } from 'lucide-react';

interface Props {
  children: ReactNode;
}
interface State {
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('[ErrorBoundary]', error, info.componentStack);
  }

  render() {
    if (this.state.error) {
      return (
        <div className="flex items-center justify-center min-h-[400px]">
          <div className="card max-w-md text-center">
            <AlertTriangle size={36} className="mx-auto text-destructive mb-3" />
            <h3 className="font-semibold text-lg mb-2">页面渲染出错</h3>
            <p className="text-sm text-muted-foreground mb-4">
              {this.state.error.message}
            </p>
            <button
              className="btn btn-primary"
              onClick={() => this.setState({ error: null })}
            >
              重试
            </button>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}
