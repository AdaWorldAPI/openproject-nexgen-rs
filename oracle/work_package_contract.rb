# frozen_string_literal: true

# Records the behaviour of OpenProject's own WorkPackages::CreateContract and
# WorkPackages::UpdateContract on factory-built records.
#
#   cd <openproject checkout> && bin/rails runner <this file>
#
# Output path: ENV["OP_TRACE_OUT"], default work-package-contract-trace.json next to this file.

require "json"
require "factory_bot"
require "test_prof"
require "test_prof/factory_default"
TestProf::FactoryDefault.init # the factories call TestProf::FactoryBot.get_factory_default

started = Process.clock_gettime(Process::CLOCK_MONOTONIC)

STUBS = [
  "ActionMailer::Base.delivery_method = :test (no mail is delivered)",
  "ActiveJob::Base.queue_adapter = :test (no job is performed)",
  "Rails.cache replaced by ActiveSupport::Cache::NullStore",
  "RequestStore cleared between cases"
].freeze

ActionMailer::Base.delivery_method = :test
ActionMailer::Base.perform_deliveries = false
ActiveJob::Base.queue_adapter = :test
Rails.cache = ActiveSupport::Cache::NullStore.new

root = Rails.root
FactoryBot.definition_file_paths = [root.join("spec/factories").to_s] +
                                   Dir[root.join("modules/*/spec/factories").to_s]
FactoryBot.find_definitions
include FactoryBot::Syntax::Methods # rubocop:disable Style/MixinUsage

PERMISSIONS = %i[
  edit_work_packages add_work_packages assign_versions manage_subtasks
  change_work_package_status view_project_phases log_time move_work_packages
  view_work_packages
].freeze

def declared_attributes(contract)
  klass = contract.class
  names = contract.send(:collect_ancestor_attributes, :writable_attributes) +
          contract.send(:collect_ancestor_attributes, :writable_conditions).keys +
          contract.send(:collect_ancestor_attributes, :attribute_permissions).keys.map(&:to_s)
  names = names.map(&:to_s).reject { |n| n == "default_permission" }
  names += names.map { |n| n.delete_suffix("_id") }
  names += names.map { |n| "#{n}_id" if !n.end_with?("_id") }.compact
  names += contract.send(:ancestor_attribute_aliases).values.map(&:to_s)
  _ = klass
  names.uniq.sort
end

def facts_for(wp)
  {
    "leaf?" => wp.leaf?,
    "schedule_manually" => wp.schedule_manually,
    "status.closed?" => wp.status&.closed?,
    "effective_target_versions.any?(&:closed?)" => wp.effective_target_versions.any?(&:closed?),
    "target_versions.any?(&:closed?)" => wp.target_versions.any?(&:closed?),
    "status_id_changed?" => wp.status_id_changed?,
    "status_id_change_present" => !wp.status_id_change.nil?,
    "persisted?" => wp.persisted?,
    "children.any?" => wp.children.any?
  }
end

def record(contract_class, wp, user, label)
  contract = contract_class.new(wp, user)
  writable = declared_attributes(contract).to_h { |a| [a, contract.writable?(a)] }
  valid = contract.valid?
  {
    "contract" => contract_class.name,
    "user" => label,
    "facts" => facts_for(wp),
    "writable" => writable,
    "valid" => valid,
    "errors" => contract.errors.details.to_h { |k, v| [k.to_s, v.map { |d| d.transform_values { |x| x.is_a?(Symbol) ? x.to_s : x } }] }
  }
end

def new_from(wp)
  WorkPackage.new(
    project: wp.project, type: wp.type, priority: wp.priority, status: wp.status,
    author: wp.author, subject: wp.subject, schedule_manually: wp.schedule_manually
  ).tap do |n|
    ids = wp.target_versions.map(&:id)
    n.target_version_ids_replacements = ids if ids.any?
  end
end

def member_user(project, permissions = PERMISSIONS)
  create(:user, member_with_permissions: { project => permissions })
end

CASES = {
  "leaf_manual" => lambda {
    wp = create(:work_package, schedule_manually: true)
    [wp, member_user(wp.project), "member", true]
  },
  "leaf_automatic" => lambda {
    wp = create(:work_package, schedule_manually: false)
    [wp, member_user(wp.project), "member", true]
  },
  "parent_automatic" => lambda {
    wp = create(:work_package, schedule_manually: false)
    create(:work_package, project: wp.project, parent: wp, schedule_manually: false)
    [wp.reload, member_user(wp.project), "member", false]
  },
  "parent_manual" => lambda {
    wp = create(:work_package, schedule_manually: true)
    create(:work_package, project: wp.project, parent: wp, schedule_manually: false)
    [wp.reload, member_user(wp.project), "member", false]
  },
  "closed_version_closed_status" => lambda {
    project = create(:project_with_types)
    version = create(:version, project:, status: "closed")
    wp = create(:work_package, project:, status: create(:closed_status), version:)
    [wp.reload, member_user(project), "member", true]
  },
  "closed_version_closed_status_changed" => lambda {
    project = create(:project_with_types)
    version = create(:version, project:, status: "closed")
    wp = create(:work_package, project:, status: create(:closed_status), version:)
    wp = WorkPackage.find(wp.id)
    wp.status = create(:status)
    [wp, member_user(project), "member", true]
  },
  "no_permissions" => lambda {
    wp = create(:work_package, schedule_manually: true)
    [wp, member_user(wp.project, []), "member_no_permissions", true]
  },
  "admin" => lambda {
    wp = create(:work_package, schedule_manually: true)
    [wp, create(:admin), "admin", true]
  }
}.freeze

cases = CASES.map do |name, builder|
  result = nil
  ActiveRecord::Base.transaction do
    RequestStore.clear! if defined?(RequestStore)
    User.current = nil
    wp, user, label, create_meaningful = builder.call
    entries = [record(WorkPackages::UpdateContract, wp, user, label)]
    entries << record(WorkPackages::CreateContract, new_from(wp), user, label) if create_meaningful
    result = entries.map { |e| e.merge("name" => name) }
    raise ActiveRecord::Rollback
  end
  result
end.flatten

def sort_keys(o)
  case o
  when Hash then o.sort_by { |k, _| k.to_s }.to_h { |k, v| [k.to_s, sort_keys(v)] }
  when Array then o.map { |v| sort_keys(v) }
  else o
  end
end

trace = sort_keys(
  "source" => {
    "repo" => "AdaWorldAPI/openproject",
    "rev" => `git -C #{root} rev-parse HEAD`.strip,
    "ruby" => RUBY_VERSION,
    "rails" => Rails.version
  },
  "stubs" => STUBS,
  "cases" => cases
)

out = ENV.fetch("OP_TRACE_OUT") { File.join(__dir__, "work-package-contract-trace.json") }
File.write(out, "#{JSON.pretty_generate(trace)}\n")
elapsed = Process.clock_gettime(Process::CLOCK_MONOTONIC) - started
warn "wrote #{out} (#{cases.size} records, #{elapsed.round(1)}s after boot)"
