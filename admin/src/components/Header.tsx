import React from 'react';
import { ShieldCheck, Activity, Send } from 'lucide-react';

interface HeaderProps {
  isOnline: boolean;
  onSave: () => void;
  isSaving: boolean;
}

export const Header: React.FC<HeaderProps> = ({ isOnline, onSave, isSaving }) => {
  return (
    <header className="app-header">
      <div className="header-left">
        <div className="brand-logo">
          <div className="brand-icon">
            <ShieldCheck size={20} color="#fff" />
          </div>
          <span>AegisCMS</span>
        </div>
        <span className="brand-badge">Zero-Trust Sandbox</span>
      </div>

      <div className="header-right">
        <div className="health-status">
          <span className={`status-dot ${isOnline ? 'online' : ''}`} />
          <span>Core Gateway: {isOnline ? 'Connected' : 'Offline'}</span>
        </div>

        <button 
          className="btn btn-primary" 
          onClick={onSave}
          disabled={isSaving}
          title="Save content and trigger Wasm guest hooks"
        >
          {isSaving ? (
            <Activity className="animate-spin" size={16} />
          ) : (
            <Send size={16} />
          )}
          <span>{isSaving ? 'Executing Hooks...' : 'Save & Run Hooks'}</span>
        </button>
      </div>
    </header>
  );
};
