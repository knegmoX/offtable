import { render } from 'preact';
import './app/theme.css';
import { App } from './app/App';
import { locale } from './app/i18n';

document.documentElement.lang = locale.value;
render(<App />, document.getElementById('app')!);
