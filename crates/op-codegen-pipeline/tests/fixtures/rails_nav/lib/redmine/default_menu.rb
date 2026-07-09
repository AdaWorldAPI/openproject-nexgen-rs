Redmine::MenuManager.map :application_menu do |menu|
  menu.push :work_packages, { controller: 'work_packages', action: 'index' }
  menu.push :projects, { controller: 'projects', action: 'index' }
end
