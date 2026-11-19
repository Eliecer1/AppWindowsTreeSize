; Al desinstalar, quita "Analizar con AppWindowsTreeSize" del clic derecho del Explorador.
!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegKey HKCU "Software\Classes\Directory\shell\AppWindowsTreeSize"
  DeleteRegKey HKCU "Software\Classes\Drive\shell\AppWindowsTreeSize"
!macroend
