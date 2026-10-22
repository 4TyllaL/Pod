param([string]$Dir, [int]$X, [int]$Y)
# Janela de "outro programa" para o teste de foco: registra ativacao, foco do campo de texto e o que foi digitado.
Add-Type -AssemblyName System.Windows.Forms
$log = Join-Path $Dir 'helper.log'
function L($m) { Add-Content -Path $log -Value ("{0:HH:mm:ss.fff} {1}" -f (Get-Date), $m) }
$form = New-Object System.Windows.Forms.Form
$form.Text = 'foco-teste'; $form.Width = 420; $form.Height = 220; $form.StartPosition = 'Manual'; $form.Left = $X; $form.Top = $Y
$tb = New-Object System.Windows.Forms.TextBox; $tb.Dock = 'Fill'; $tb.Multiline = $true; $form.Controls.Add($tb)
$form.Add_Activated({ L 'form ATIVADO' })
$form.Add_Deactivate({ L 'form DESATIVADO' })
$tb.Add_GotFocus({ L 'campo de texto GANHOU foco' })
$tb.Add_LostFocus({ L 'campo de texto PERDEU foco' })
$tb.Add_TextChanged({ Set-Content -Path (Join-Path $Dir 'text.txt') -Value $tb.Text -Encoding ascii })
$form.Add_Shown({ $tb.Focus(); Set-Content -Path (Join-Path $Dir 'hwnd.txt') -Value $form.Handle.ToInt64(); L 'form mostrado' })
$t = New-Object System.Windows.Forms.Timer; $t.Interval = 250
$t.Add_Tick({ if (Test-Path (Join-Path $Dir 'quit.txt')) { $form.Close() } })
$t.Start()
[System.Windows.Forms.Application]::Run($form)
