I am trying to run `cmd.exe` in `portable-pty` on Windows (using GitHub actions, because I am to poor to buy a copy of Windows). Additionally I am not a fan of Windows. But I still want my features to be available to as many people as possible so I have to deal with it. But I do not have Windows so I have to do cumbersome work using slow GitHub actions to test the behavior of Windows.

@GitHub it would be much easier and probably less CPU-time intensive to just provide a virtual machine or something for intactive low latency and high frequency testing of Microsoft features (bugs) instead of high latency and high-CPU-usage-GitHub-actions-missuse.

There are generally 2 main distinctions between usage of portable-pty:

1. spawning interactive terminal and interact with it
2. spawning a one-time command

The problems of spawning an interactive terminal is a superset of spawning a one-time command.

## First hurdle
The first hurdle that you get when trying to run `cmd.exe` or similar in `portable-pty` is the following prompt:

```text
0	27	
1	91	[
2	54	6
3	110	n
```

See
- [GitHub action](https://github.com/MBanucu/portable-pty-fixed/actions/runs/21918639249/job/63292559395?pr=1)
- [PR](https://github.com/MBanucu/portable-pty-fixed/pull/1).

This prompt is a request for the user/terminal to specify the inital cursor position on the screen. To get around this issue, one has to specify the initial cursor possition using the writer, for example by writing the following bytes to the pipe:

```
"\x1b[1;1R"
```

This will set the initial cursor position to the corrdinates `x = 1, y = 1` in the top left corner of the screen.

### Why?
Why does this happen?

### Because!
Because of the following `feature`: `PSEUDOCONSOLE_INHERIT_CURSOR`, see [CreatePseudoConsole function](https://learn.microsoft.com/en-us/windows/console/createpseudoconsole).

### Solutions
1. Be chatty and chat a little bit with the ConPTY and tell it your deepest desires, for example, where you want the initial cursor position to be.
2. Disable the bug! How to disable the bug that is a feature? You clone the `wezterm` repo, remove the hard coded flag, set up a rust development environment, compile it from source and probably republish the library becaus you want to use it as a base library for your own library of program. Easy. here it is:

[https://github.com/wezterm/wezterm/blob/05343b387085842b434d267f91b6b0ec157e4331/pty/src/win/psuedocon.rs#L83C1-L91C14](https://github.com/wezterm/wezterm/blob/05343b387085842b434d267f91b6b0ec157e4331/pty/src/win/psuedocon.rs#L83C1-L91C14)

Modify this
```
            (CONPTY.CreatePseudoConsole)(
                size,
                input.as_raw_handle() as _,
                output.as_raw_handle() as _,
                PSUEDOCONSOLE_INHERIT_CURSOR
                    | PSEUDOCONSOLE_RESIZE_QUIRK
                    | PSEUDOCONSOLE_WIN32_INPUT_MODE,
                &mut con,
            )
```
to that
```
            (CONPTY.CreatePseudoConsole)(
                size,
                input.as_raw_handle() as _,
                output.as_raw_handle() as _,
                0
                &mut con,
            )
```

If you know what you are doing, you can add `PSEUDOCONSOLE_RESIZE_QUIRK` and `PSEUDOCONSOLE_WIN32_INPUT_MODE` to your needs, but I do not yet. Maybe some time in the future. Better solution: make it configurable for the user of the library and document the usage, because there is not a lot of documentation about this at all and it costs a lot of time to get to know the bugs that get introduced by these features. As far as I can see, these two features are either not implemented or not officially supported.

Notice the misspelling of `PSUEDOCONSOLE_INHERIT_CURSOR` instead of `PSEUDOCONSOLE_INHERIT_CURSOR` as stated on the official [docs](https://learn.microsoft.com/en-us/windows/console/createpseudoconsole) but this is not that much of a problem in the code because the constant is not fetched from a Microsoft library but instead defined as constant `pub const PSUEDOCONSOLE_INHERIT_CURSOR: DWORD = 0x1;` in the same file. Even the file is named `psuedocon.rs` instead of `pseudocon.rs`, but the `wezterm` developers are not event consistent in their naming. Sometimes it is named `pseudocon` and sometimes `psuedocon`. What does this all mean? It means: The `wezterm` developers do not give a fuck about this sub-library. They only care about wezterm as a whole and wezterm does not need to care about details of this library and if it doesn't work on Windows, who cares? The library is working very well on Linux and macOS, but it may be a little bit unstable on Windows, but the instability is only in rare cases so they do not give a fuck. It is a special case of a special case.

So there are these indicia that lead to the conclusion of wezterm-developer-not-giving-a-fuck:
1. Introduction of features like `PSEUDOCONSOLE_INHERIT_CURSOR` that break everything instead of being useful. The developers probably didn't read the [docs](https://learn.microsoft.com/en-us/windows/console/createpseudoconsole) because they are lazy or something and the "feature" that is called `PSEUDOCONSOLE_INHERIT_CURSOR` sounds good, so they used it, but never actually tested it.
2. There is not a single test on their `wezterm` repo that tests this behavior of spawning `cmd.exe` or `powershell.exe` or anything like that using `portable-pty`.
3. Inconsisten naming of variables and files (`psuedocon` vs. `pseudocon`).
4. The last modification/update of the `portably-pty` library for Windows support was 2 years ago.

## ConPTY
It seems like the behavior of ConPTY of Windows is changing frequently. The current behavior is:

### Drop the writer before child exited
If you drop the writer before child exited then the child will be exited with `STATUS_CONTROL_C_EXIT` (`0xC000013A` = `Ctrl+C`). The reader pipe is closing with the message from the `ConPTY`:
```
0	27	
1	91	[
2	63	?
3	57	9
4	48	0
5	48	0
6	49	1
7	108	l
8	27	
9	91	[
10	63	?
11	49	1
12	48	0
13	48	0
14	52	4
15	108	l
```
The meaning of this is (quote from Grok):

> The provided data represents two concatenated ANSI escape sequences in a terminal control context:
> 
> 1. **First sequence**: `\x1b[?9001l` (ESC [ ? 9 0 0 1 l)  
>    - This is a private mode reset command specific to Windows Terminal (Microsoft's conhost or Windows Terminal emulator).  
>    - It disables "Win32 Input Mode" (?9001 l), a non-standard extension where the terminal sends raw Win32-style input events (e.g., keyboard, mouse, clipboard pastes) encoded as special escape sequences to the application for advanced input handling. This mode allows legacy Win32 console apps to receive richer input via VT/ANSI protocols, but it's typically enabled only when needed for compatibility.
> 
> 2. **Second sequence**: `\x1b[?1004l` (ESC [ ? 1 0 0 4 l)  
>    - This is a DEC private mode reset command (widely supported in xterm-compatible terminals, including Windows Terminal).  
>    - It disables "focus event reporting" (?1004 l), which stops the terminal from sending notifications (ESC [ I for focus in, ESC [ O for focus out) when the terminal window gains or loses focus.
> 
> These sequences are often used together at the end of a terminal session, script, or application (e.g., in tools like Vim, tmux, or custom console apps) to restore default input behavior and clean up after enabling special modes for mouse tracking, focus events, or enhanced input. For example, they might appear when exiting an application that temporarily enabled these features for better interactivity.

See
- [GitHub action](https://github.com/MBanucu/portable-pty-fixed/actions/runs/21918807635/job/63293148222?pr=3)
- [PR](https://github.com/MBanucu/portable-pty-fixed/pull/3).

### Drop the master before child exited
If you drop the master before child exited then it is the same behavior as dropping the master too early.

- The message on the reader is:
```
0	27	
1	91	[
2	63	?
3	57	9
4	48	0
5	48	0
6	49	1
7	108	l
8	27	
9	91	[
10	63	?
11	49	1
12	48	0
13	48	0
14	52	4
15	108	l
```
- The exit code is: `3221225786` = `0xC000013A` = `STATUS_CONTROL_C_EXIT` = `Ctrl+C`.
- The reader pipe closes.

See
- [GitHub action](https://github.com/MBanucu/portable-pty-fixed/actions/runs/21918876088/job/63293388162?pr=4)
- [PR](https://github.com/MBanucu/portable-pty-fixed/pull/4)

## How to close the reader pipe properly

Observation:
- [drop master only](https://github.com/MBanucu/portable-pty-fixed/tree/refs/heads/drop-master-only) is passing the test, so it is closing the reader pipe.
- [drop writer only](https://github.com/MBanucu/portable-pty-fixed/tree/refs/heads/drop-writer-only) is passing the test, so it is closing the reader pipe.
- [drop nothing](https://github.com/MBanucu/portable-pty-fixed/tree/refs/heads/drop-nothing) is not passing the test with a timeout waiting for the reader thread to finish.

So if you are on Linux or macOS then you do not have to do anything. If you run `bash` or `sh` then the termination of `bash` and `sh` will close the pipe automatically and no manual dropping measures have to be taken to close the pipe and be sure to get all the last bits of information out of the reader.

The only problem is if you have to deal with Windows (`ConPTY`). Then the sequence is as follows:
- Make sure that the child exited, either by polling or by block-waiting.
- Drop the master or the writer to close the reader pipe.

Hopefully there is only set an EOF signal at the end of the reader pipe by dropping master or writer and not signaling something like `STATUS_CONTROL_C_EXIT` to the pipe to make sure that a slow reader thread that has not yet fetched all the last bits of the reader pipe can read the pipe to the end. I will probably soon make this test with a thread that is on purpose slow and test if this race condition exists and spits into the soup or not.