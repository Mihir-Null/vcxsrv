param(
    [Parameter(Mandatory)][string]$UnixEndpoint,
    [string]$Authority,
    [string]$Display='1',
    [switch]$WrongCookie,
    [double]$TimeoutSeconds=5
)
$ErrorActionPreference='Stop'
if($TimeoutSeconds -le 0) { throw 'Timeout must be positive' }

function Read-AuthorityCookie([string]$Path,[string]$Selector) {
    $stream=[IO.File]::OpenRead($Path)
    try {
        while($stream.Position -lt $stream.Length) {
            $family=[byte[]]::new(2)
            $stream.ReadExactly($family,0,2)
            $fields=[Collections.Generic.List[byte[]]]::new()
            for($i=0;$i -lt 4;$i++) {
                $length=[byte[]]::new(2)
                $stream.ReadExactly($length,0,2)
                $size=([int]$length[0] -shl 8)+[int]$length[1]
                $value=[byte[]]::new($size)
                $stream.ReadExactly($value,0,$size)
                $fields.Add($value)
            }
            if([Text.Encoding]::ASCII.GetString($fields[1]) -ceq $Selector -and
                [Text.Encoding]::ASCII.GetString($fields[2]) -ceq 'MIT-MAGIC-COOKIE-1' -and
                $fields[3].Length -eq 16) {
                return ,$fields[3]
            }
        }
        throw 'No matching authority record'
    } finally { $stream.Dispose() }
}

function Read-SocketExact([Net.Sockets.Socket]$Socket,[int]$Size,[long]$Deadline) {
    $bytes=[byte[]]::new($Size)
    $offset=0
    while($offset -lt $Size) {
        $remaining=($Deadline-[Diagnostics.Stopwatch]::GetTimestamp())*1000.0/[Diagnostics.Stopwatch]::Frequency
        if($remaining -le 0) { throw 'Setup deadline expired' }
        $Socket.ReceiveTimeout=[int][Math]::Max(1,[Math]::Ceiling($remaining))
        $read=$Socket.Receive($bytes,$offset,$Size-$offset,[Net.Sockets.SocketFlags]::None)
        if($read -eq 0) { throw 'Incomplete setup response' }
        $offset+=$read
    }
    return ,$bytes
}

$connection=$null
try {
    $cookie=[byte[]]::new(0)
    $name=[byte[]]::new(0)
    if($Authority) {
        $cookie=Read-AuthorityCookie $Authority $Display
        $name=[Text.Encoding]::ASCII.GetBytes('MIT-MAGIC-COOKIE-1')
    }
    if($WrongCookie) {
        if($cookie.Length -ne 16) { throw 'Wrong-cookie needs authority input' }
        $cookie[0]=$cookie[0] -bxor 1
    }
    $request=[Collections.Generic.List[byte]]::new()
    $request.AddRange([byte[]]@(108,0,11,0,0,0,$name.Length,0,$cookie.Length,0,0,0))
    $request.AddRange($name)
    for($i=0;$i -lt ((4-$name.Length%4)%4);$i++) {$request.Add(0)}
    $request.AddRange($cookie)
    for($i=0;$i -lt ((4-$cookie.Length%4)%4);$i++) {$request.Add(0)}
    $payload=$request.ToArray()
    $deadline=[long]([Diagnostics.Stopwatch]::GetTimestamp()+$TimeoutSeconds*[Diagnostics.Stopwatch]::Frequency)
    $connection=[Net.Sockets.Socket]::new([Net.Sockets.AddressFamily]::Unix,
        [Net.Sockets.SocketType]::Stream,[Net.Sockets.ProtocolType]::Unspecified)
    $cancel=[Threading.CancellationTokenSource]::new([timespan]::FromSeconds($TimeoutSeconds))
    try {
        $null=$connection.ConnectAsync([Net.Sockets.UnixDomainSocketEndPoint]::new($UnixEndpoint),
            $cancel.Token).AsTask().GetAwaiter().GetResult()
    } finally { $cancel.Dispose() }
    $offset=0
    while($offset -lt $payload.Length) {
        $remaining=($deadline-[Diagnostics.Stopwatch]::GetTimestamp())*1000.0/[Diagnostics.Stopwatch]::Frequency
        if($remaining -le 0) { throw 'Setup deadline expired' }
        $connection.SendTimeout=[int][Math]::Max(1,[Math]::Ceiling($remaining))
        $sent=$connection.Send($payload,$offset,$payload.Length-$offset,[Net.Sockets.SocketFlags]::None)
        if($sent -eq 0) { throw 'Incomplete setup request' }
        $offset+=$sent
    }
    $header=Read-SocketExact $connection 8 $deadline
    $body=Read-SocketExact $connection ([BitConverter]::ToUInt16($header,6)*4) $deadline
    if($header[0] -eq 1 -and [BitConverter]::ToUInt16($header,2) -eq 11 -and $body.Length -ge 32) {
        Write-Output 'ACCEPTED'; exit 0
    }
    if($header[0] -eq 0) { Write-Output 'REJECTED'; exit 1 }
    throw 'Unexpected setup response'
} catch {
    # Exception contents may include paths/fields; print no credential material.
    Write-Output ('PROBE_ERROR: '+$_.Exception.GetType().Name)
    exit 2
} finally {
    if($connection) { $connection.Dispose() }
}
