import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './app';
import { AuthProvider } from '../infrastructure/auth/auth-provider';
import { AuthGuard } from './auth-guard';
import './styles.css';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <AuthProvider>
      <AuthGuard>
        <App />
      </AuthGuard>
    </AuthProvider>
  </React.StrictMode>
);
