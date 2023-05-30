Set-StrictMode -Version Latest

<#
# 花括号包裹的叫做 script-block，ToString() 命令将其转换为字符串
# 修改窗口的字符集到 UTF8
# 确定我们要使用的 MSYS 工具路径
# 将 MSYS 提供的路径添加到环境变量中
# 确定 Rust 工具链
#>

$common_cmd_string = {
    $MSYS_DIR = ${Env:msys_dir}
    chcp 65001 | Out-Null
    $tools_top_dir = "${MSYS_DIR}\ucrt64"
    Set-Item -Path Env:PATH -Value "${tools_top_dir}\bin;${MSYS_DIR}\usr\bin;${Env:PATH}"
    Set-Item -Path Env:RUSTUP_TOOLCHAIN -Value "stable-x86_64-pc-windows-gnu"
}.ToString()


# 然后我们逐个将要输入的后面的内容追加在命令行字符串上，注意添加一个换行符 `n

$top_left_cmd_string = $common_cmd_string + "`n" + { openocd }.ToString()

# 然后将完整的命令字符串转换为字节串

$top_left_cmd_bytes = [System.Text.Encoding]::Unicode.GetBytes( $top_left_cmd_string )

# 字节串用 Base64 编码，以方便输出给 powershell 的 EncodedCommand 命令

$top_left_cmd_enc = [Convert]::ToBase64String( $top_left_cmd_bytes )

$top_right_cmd_string = $common_cmd_string
$top_right_cmd_bytes = [System.Text.Encoding]::Unicode.GetBytes( $top_right_cmd_string )
$top_right_cmd_enc = [Convert]::ToBase64String( $top_right_cmd_bytes )

$bottom_left_cmd_string = $common_cmd_string + "`n" + { telnet 127.0.0.1 8888 }.ToString()
$bottom_left_cmd_bytes = [System.Text.Encoding]::Unicode.GetBytes( $bottom_left_cmd_string )
$bottom_left_cmd_enc = [Convert]::ToBase64String( $bottom_left_cmd_bytes )

$bottom_right_cmd_string = $common_cmd_string + "`n" + { telnet 127.0.0.1 4444 }.ToString()
$bottom_right_cmd_bytes = [System.Text.Encoding]::Unicode.GetBytes( $bottom_right_cmd_string )
$bottom_right_cmd_enc = [Convert]::ToBase64String( $bottom_right_cmd_bytes )

Set-Location ${PSScriptRoot}

<#
# 创建一个主窗口，主窗口运行 OpenOCD
# 向左切分一个面板，让这个面板运行一个 PowerShell，方便我们执行 shell 命令（比如启动 VSCode）
# 从左侧面板向下切分一个面板，这个面板通过 telnet 运行 RTT 显示
# 从右侧面板向下切分一个面板，这个面板通过 telnet 连接 OpenOCD，以便直接控制 OpenOCD 的运行
#>

wt -d ${PSScriptRoot} pwsh.exe -ExecutionPolicy ByPass -NoExit -EncodedCommand $top_left_cmd_enc `; `
split-pane -d ${PSScriptRoot} pwsh.exe -ExecutionPolicy ByPass -NoExit -EncodedCommand $top_right_cmd_enc `; `
move-focus left `; split-pane -d ${PSScriptRoot} pwsh.exe -ExecutionPolicy ByPass -NoExit -EncodedCommand $bottom_left_cmd_enc `; `
move-focus right `; split-pane -d ${PSScriptRoot} pwsh.exe -ExecutionPolicy ByPass -NoExit -EncodedCommand $bottom_right_cmd_enc `; `
move-focus up
