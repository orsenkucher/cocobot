Get-ChildItem -Recurse | `
    Where-Object { $_.Name -eq '.gitignore' } | `
    ForEach-Object { `
        Write-Host $_.DirectoryName; `
        Push-Location $_.DirectoryName; `
        git status --ignored --short | `
            Where-Object { $_.StartsWith('!! ') } | `
            ForEach-Object { `
                $ignore = $_.Split(' ')[1].Trim('/'); `
                Write-Host $ignore; `
                Set-Content -Path $ignore -Stream com.dropbox.ignored -Value 1 `
            }; `
        Pop-Location }
