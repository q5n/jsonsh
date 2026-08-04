#!/bin/bash

majorAdd=0
minorAdd=0
patchAdd=0

case "$1" in
  +001)
    patchAdd=1
    ;;
  +010)
    minorAdd=1
    ;;
  +100)
    majorAdd=1
    ;;
  *)
    echo "arg format error! only support +001/+010/+100 "
    exit 1
    ;;
esac

lastVerTag=`git tag -l --sort=v:refname |grep -E "^v[0-9]+\.[0-9]+\.[0-9]+$"|tail -1`

if [ -z "$lastVerTag" ];then
   patch=0  minor=0  major=0
else
    IFS=. read -r major minor patch <<< "${lastVerTag#v}"
fi

if [ $patchAdd -eq 1 ]; then
   let patch+=1
fi
if [ $minorAdd -eq 1 ]; then
   patch=0
   let minor+=1
fi
if [ $majorAdd -eq 1 ]; then
   patch=0
   minor=0
   let major+=1
fi

echo "lastVerTag: $lastVerTag"
nextVer="${major}.${minor}.${patch}"
nextVerTag="v$nextVer"

echo "nextVerTag: $nextVerTag"
git add -A
git commit -m "release $nextVerTag"

# 保留20个版本tag，删除旧tag（本地+远程）
clean_old_tags(){
    local keep=20
    echo "clean old tags, keep latest $keep tags..."
    tags=( $(git tag -l --sort=-v:refname | grep -E "^v[0-9]+\.[0-9]+\.[0-9]+$") )

    if [ ${#tags[@]} -le $keep ]; then
        echo "tag count ${#tags[@]}, no cleanup needed"
        return
    fi

    old_tags=( "${tags[@]:$keep}" )

    for tag in "${old_tags[@]}"; do
        echo "delete tag: $tag"
        # 删除本地tag
        git tag -d "$tag"
        # 删除GitHub远程tag
        git push origin --delete tag "$tag"
    done
}
clean_old_tags

git push
git tag $nextVerTag
git push origin $nextVerTag



