const fs = require('fs');
const path = require('path');

function isSkillDocument(relativePath) {
    const parts = relativePath.split('/');
    const name = parts[parts.length - 1].toLowerCase();
    if (name === 'skill.md') return true;
    return name.endsWith('.md') && parts.includes('generated');
}

function skillName(relativePath) {
    const parts = relativePath.split('/');
    const name = parts[parts.length - 1];
    if (name.toLowerCase() === 'skill.md') return parts[parts.length - 2] || null;
    return name.replace(/\.md$/i, '');
}

function listSkillDocuments(skillsRoot) {
    const root = path.resolve(skillsRoot);
    const documents = [];
    const pending = [''];
    while (pending.length > 0) {
        const relativeDir = pending.pop();
        let entries;
        try {
            entries = fs.readdirSync(path.join(root, relativeDir), { withFileTypes: true });
        } catch {
            continue;
        }
        for (const entry of entries) {
            if (entry.name.startsWith('.')) continue;
            const relativePath = relativeDir ? `${relativeDir}/${entry.name}` : entry.name;
            if (entry.isDirectory()) {
                pending.push(relativePath);
            } else if (entry.isFile() && isSkillDocument(relativePath)) {
                const name = skillName(relativePath);
                if (name) documents.push({ path: relativePath, name });
            }
        }
    }
    return documents.sort((left, right) => left.name.localeCompare(right.name) || left.path.localeCompare(right.path));
}

module.exports = { listSkillDocuments };
