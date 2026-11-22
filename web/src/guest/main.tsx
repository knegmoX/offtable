import { render } from 'preact';
import '../app/theme.css';
import { GuestApp } from './GuestApp';
import { locale } from '../app/i18n';

document.documentElement.lang = locale.value;
render(<GuestApp />, document.getElementById('app')!);
